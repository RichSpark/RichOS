import AppKit
import ApplicationServices
import Carbon.HIToolbox
import Darwin
import RichFilesCore

final class GestureController {
    private typealias Preferences = WindowGesturePreferences
    // Layouts with tiling margins and character grids can land just inside
    // the requested edges while still filling the display or the smaller frame.
    private static let layoutTolerance: CGFloat = 16
    private struct RestorableFrame {
        let original: CGRect
        let applied: CGRect
        let display: CGDirectDisplayID
    }
    private struct TileDrag {
        let window: AXUIElement
        let origin: CGPoint
        let original: CGRect
        var moved = false
    }
    private enum DockClick {
        case minimize(AXUIElement)
        case restore(AXUIElement)
    }
    private enum Target {
        case dock(NSRunningApplication, DockClick)
        case title(AXUIElement, Int)
    }
    private struct Press {
        let point: CGPoint
        let timestamp: UInt64
        let target: Target
    }

    private let system = AXUIElementCreateSystemWide()
    private var tap: CFMachPort?
    private var source: CFRunLoopSource?
    private var selection: Preferences.Selection = []
    private var reportedStatus: Preferences.Status?
    private var press: Press?
    private var pointerRevision: UInt64 = 0
    private var firstTitleClick: AXUIElement?
    private var maximized: [AXUIElement: RestorableFrame] = [:]
    private var tiled: [AXUIElement: RestorableFrame] = [:]
    private var tileDrag: TileDrag?
    private var dockMinimized: [pid_t: AXUIElement] = [:]
    private var observers: [pid_t: AXObserver] = [:]
    private var appTermination: NSObjectProtocol?
    private var accessibilityChanges: NSObjectProtocol?
    private var recheckingPermissions = false
    private var permissionCheckQueued = false
    private var permission: GesturePermission?

    init() {
        AXUIElementSetMessagingTimeout(system, 0.08)
        appTermination = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didTerminateApplicationNotification, object: nil, queue: .main
        ) { [weak self] notification in
            guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { return }
            self?.forget(process: app.processIdentifier)
        }
        // HIServices can hold its trust verdict for the lifetime of a process.
        // A privacy change must invalidate the tap before we use that verdict again.
        accessibilityChanges = DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name("com.apple.accessibility.api"), object: nil, queue: .main
        ) { [weak self] _ in self?.recheckPermissions() }
        CFNotificationCenterAddObserver(
            CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(self).toOpaque(),
            { _, observer, _, _, _ in
                guard let observer else { return }
                let controller = Unmanaged<GestureController>.fromOpaque(observer).takeUnretainedValue()
                DispatchQueue.main.async { controller.recheckPermissions() }
            }, "com.apple.tcc.access.changed" as CFString, nil, .deliverImmediately
        )
    }

    func refresh() {
        guard !recheckingPermissions else { return }
        let previous = selection
        selection = Preferences.selection
        if !selection.contains(.titleBar) && previous.contains(.titleBar) { maximized.removeAll() }
        if !selection.contains(.dock) && previous.contains(.dock) { dockMinimized.removeAll() }
        if !selection.contains(.tiling) && previous.contains(.tiling) { tiled.removeAll() }
        if selection != previous {
            stopTap(keepingWindows: true)
            pruneObservers()
        }

        let status: Preferences.Status
        if selection.isEmpty {
            status = .off
        } else {
            let measured = GesturePermission.current
            permission = measured
            switch measured {
            case .accessibility: status = .accessibility
            case .inputMonitoring: status = .inputMonitoring
            case .granted: status = startTap() ? .running : .inputUnavailable
            }
        }

        if status != .running { stopTap() }
        if status != reportedStatus {
            reportedStatus = status
            Preferences.publish(status)
            record(.windowGestureAvailability, status == .running || status == .off ? .complete : .unavailable)
        }
    }

    private func recheckPermissions() {
        guard !Preferences.selection.isEmpty else { return }
        if recheckingPermissions { permissionCheckQueued = true; return }
        recheckingPermissions = true
        stopTap(keepingWindows: true)
        // Both notifications can describe a grant for a different app. A fresh
        // process can check them without discarding the windows we remember.
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(150)) { [weak self] in
            self?.probePermission()
        }
    }

    private func probePermission() {
        guard let executable = Bundle.main.executableURL else { finishPermissionCheck(nil); return }
        let probe = Process()
        probe.executableURL = executable
        probe.arguments = ["--check-gesture-permission"]
        probe.standardOutput = FileHandle.nullDevice
        probe.standardError = FileHandle.nullDevice
        probe.terminationHandler = { [weak self] result in
            let observed = result.terminationReason == .exit ? GesturePermission(rawValue: result.terminationStatus) : nil
            DispatchQueue.main.async { self?.finishPermissionCheck(observed) }
        }
        do { try probe.run() }
        catch { finishPermissionCheck(nil); return }
        DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(2)) {
            if probe.isRunning { probe.terminate() }
        }
    }

    private func finishPermissionCheck(_ observed: GesturePermission?) {
        if Preferences.selection.isEmpty || (observed != nil && observed == permission) {
            let queued = permissionCheckQueued
            permissionCheckQueued = false
            recheckingPermissions = false
            if queued && !Preferences.selection.isEmpty { recheckPermissions() }
            else { refresh() }
            return
        }
        // exec keeps the launchd job while replacing its cached trust state.
        if let executable = Bundle.main.executableURL?.path, let argument = strdup(executable) {
            var arguments: [UnsafeMutablePointer<CChar>?] = [argument, nil]
            execv(executable, &arguments)
            free(argument)
        }
        Preferences.publish(.inputUnavailable)
        record(.windowGestureAvailability, .unavailable)
    }

    private func startTap() -> Bool {
        if let tap { return CGEvent.tapIsEnabled(tap: tap) }
        var types: [CGEventType] = []
        if !selection.intersection([.titleBar, .dock, .tiling]).isEmpty {
            types = [.leftMouseDown, .leftMouseUp, .leftMouseDragged, .rightMouseDown, .otherMouseDown]
        }
        if !selection.intersection([.desktopDelete, .tiling]).isEmpty { types.append(.keyDown) }
        let mask = types.reduce(CGEventMask(0)) { $0 | (1 << $1.rawValue) }
        // Observe mouse-down before Dock can change focus and pass through
        // mouse events unchanged. The desktop shortcut may modify a key-down.
        guard let tap = CGEvent.tapCreate(tap: .cgSessionEventTap, place: .headInsertEventTap,
                                         options: .defaultTap, eventsOfInterest: mask, callback: { _, type, event, context in
            if let context {
                let controller = Unmanaged<GestureController>.fromOpaque(context).takeUnretainedValue()
                if type == .keyDown, controller.handleWindowShortcut(event) { return nil }
                controller.receive(type, event)
            }
            return Unmanaged.passUnretained(event)
        }, userInfo: Unmanaged.passUnretained(self).toOpaque()),
              let source = CFMachPortCreateRunLoopSource(nil, tap, 0) else { return false }
        self.tap = tap
        self.source = source
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
        return true
    }

    private func stopTap(keepingWindows: Bool = false) {
        if let source { CFRunLoopRemoveSource(CFRunLoopGetMain(), source, .commonModes) }
        if let tap { CFMachPortInvalidate(tap) }
        self.source = nil
        tap = nil
        press = nil
        firstTitleClick = nil
        tileDrag = nil
        if keepingWindows { return }
        maximized.removeAll()
        tiled.removeAll()
        dockMinimized.removeAll()
        pruneObservers()
    }

    private func receive(_ type: CGEventType, _ event: CGEvent) {
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            pointerRevision &+= 1
            press = nil
            firstTitleClick = nil
            tileDrag = nil
            if let tap, AXIsProcessTrusted() { CGEvent.tapEnable(tap: tap, enable: true) }
            return
        }
        if type == .keyDown {
            applyDesktopDelete(to: event)
            return
        }
        let modifiers: CGEventFlags = [.maskCommand, .maskAlternate, .maskControl, .maskShift, .maskSecondaryFn]
        guard event.flags.intersection(modifiers).isEmpty else {
            pointerRevision &+= 1
            press = nil
            firstTitleClick = nil
            tileDrag = nil
            return
        }
        switch type {
        case .leftMouseDown:
            pointerRevision &+= 1
            // Capture activation and window identity during mouse-down, before
            // native Dock mouse-up can activate, reopen or focus another window.
            let frontmost = NSWorkspace.shared.frontmostApplication
            let clicks = Int(event.getIntegerValueField(.mouseEventClickState))
            let element = hit(at: event.location)
            press = element.flatMap { element in
                target(element, at: event.location, clicks: clicks, frontmost: frontmost)
                    .map { Press(point: event.location, timestamp: event.timestamp, target: $0) }
            }
            tileDrag = nil
            if selection.contains(.tiling), clicks == 1, let element,
               let window = titleBar(element, at: event.location), window.ordinaryWindow,
               let original = window.frame {
                tileDrag = TileDrag(window: window, origin: event.location, original: original)
            }
            if press == nil { firstTitleClick = nil }
        case .leftMouseUp:
            finishPress(event)
            finishTileDrag(at: event.location)
        case .leftMouseDragged:
            pointerRevision &+= 1
            press = nil
            firstTitleClick = nil
            if let drag = tileDrag, hypot(event.location.x - drag.origin.x, event.location.y - drag.origin.y) >= 8 {
                tileDrag?.moved = true
            }
        case .rightMouseDown, .otherMouseDown:
            pointerRevision &+= 1
            press = nil
            firstTitleClick = nil
            tileDrag = nil
        default:
            break
        }
    }

    private func handleWindowShortcut(_ event: CGEvent) -> Bool {
        let modifiers: CGEventFlags = [.maskCommand, .maskAlternate, .maskControl, .maskShift, .maskSecondaryFn]
        // Some keyboards report the Fn flag along with an arrow.
        let pressed = event.flags.intersection(modifiers).subtracting(.maskSecondaryFn)
        guard pressed == [.maskCommand, .maskAlternate, .maskControl],
              selection.contains(.tiling) else { return false }
        let key = Int(event.getIntegerValueField(.keyboardEventKeycode))
        let tile: WindowTile?
        switch key {
        case kVK_LeftArrow: tile = .left
        case kVK_RightArrow: tile = .right
        case kVK_UpArrow: tile = .top
        case kVK_DownArrow: tile = .bottom
        case kVK_ANSI_1: tile = .topLeft
        case kVK_ANSI_2: tile = .topRight
        case kVK_ANSI_3: tile = .bottomLeft
        case kVK_ANSI_4: tile = .bottomRight
        case kVK_Return: tile = nil
        default: return false
        }
        if event.getIntegerValueField(.keyboardEventAutorepeat) == 0 {
            let pid = NSWorkspace.shared.frontmostApplication?.processIdentifier
            DispatchQueue.main.async { [weak self] in
                guard let self, let pid, NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
                      let window = self.focusedWindow(pid), let current = window.frame,
                      let display = self.display(for: current) else { return }
                if let tile { self.applyTile(tile, to: window, on: display) }
                else { self.restoreTile(window, current: current, on: display) }
            }
        }
        return true
    }

    private func finishTileDrag(at point: CGPoint) {
        defer { tileDrag = nil }
        guard let drag = tileDrag, drag.moved,
              let display = GestureDisplay.current().first(where: { $0.frame.contains(point) }),
              let tile = WindowTile.atEdge(point, of: display.frame) else { return }
        let revision = pointerRevision
        DispatchQueue.main.async { [weak self] in
            guard let self, self.pointerRevision == revision else { return }
            self.applyTile(tile, to: drag.window, on: display, from: drag.original)
        }
    }

    private func display(for frame: CGRect) -> GestureDisplay? {
        let displays = GestureDisplay.current()
        func overlap(_ display: GestureDisplay) -> CGFloat {
            let area = frame.intersection(display.frame)
            return area.isNull ? 0 : area.width * area.height
        }
        return displays.max { overlap($0) < overlap($1) }
    }

    private func canTile(_ window: AXUIElement) -> Bool {
        !recheckingPermissions && Preferences.selection.contains(.tiling) && AXIsProcessTrusted()
            && CGPreflightListenEventAccess() && window.ordinaryWindow && window.minimized == false
            && window.settable(kAXPositionAttribute) && window.settable(kAXSizeAttribute)
    }

    private func applyTile(_ tile: WindowTile, to window: AXUIElement, on display: GestureDisplay, from beforeDrag: CGRect? = nil) {
        let started = ProcessInfo.processInfo.systemUptime
        guard canTile(window), let current = window.frame else {
            record(.windowGestureTile, .unavailable, started: started)
            return
        }
        let before = beforeDrag ?? current
        let previous = tiled[window]
        let original = previous.flatMap { before.nearlyEquals($0.applied) ? $0.original : nil } ?? before
        let frame = tile.frame(in: display.usable)
        let accepted = window.setFrame(frame)
        if let actual = window.frame, !actual.nearlyEquals(original) {
            tiled[window] = RestorableFrame(original: original, applied: actual, display: display.id)
            observe(window)
            record(.windowGestureTile, accepted && actual.edgesAreNear(frame, within: Self.layoutTolerance)
                ? .complete : .issues, started: started)
        } else {
            record(.windowGestureTile, .issues, started: started)
        }
    }

    private func restoreTile(_ window: AXUIElement, current: CGRect, on display: GestureDisplay) {
        let started = ProcessInfo.processInfo.systemUptime
        guard canTile(window), let saved = tiled[window], current.nearlyEquals(saved.applied) else {
            record(.windowGestureTileRestore, .unavailable, started: started)
            return
        }
        let originalDisplay = GestureDisplay.current().first { $0.usable.intersects(saved.original) }
        let target = (originalDisplay ?? display).reachable(saved.original)
        let accepted = window.setFrame(target)
        let restored = accepted && window.frame?.edgesAreNear(target, within: Self.layoutTolerance) == true
        if restored {
            tiled[window] = nil
            pruneObservers()
        }
        record(.windowGestureTileRestore, restored ? .complete : .issues, started: started)
    }

    private func applyDesktopDelete(to event: CGEvent) {
        let key = event.getIntegerValueField(.keyboardEventKeycode)
        let modifiers: CGEventFlags = [.maskCommand, .maskAlternate, .maskControl, .maskShift]
        // macOS sets Fn on the dedicated Forward Delete key.
        guard key == kVK_Delete || key == kVK_ForwardDelete,
              event.flags.intersection(modifiers).isEmpty,
              key == kVK_ForwardDelete || !event.flags.contains(.maskSecondaryFn),
              event.getIntegerValueField(.keyboardEventAutorepeat) == 0,
              Preferences.selection.contains(.desktopDelete),
              let app = NSWorkspace.shared.frontmostApplication, app.bundleIdentifier == "com.apple.finder" else { return }

        let started = ProcessInfo.processInfo.systemUptime
        let finder = AXUIElementCreateApplication(app.processIdentifier)
        AXUIElementSetMessagingTimeout(finder, 0.08)
        // Finder exposes the desktop as a focused group inside its scroll area.
        // Finder windows, Quick Look and the inline rename field have different focus.
        guard let window = finder.element(kAXFocusedWindowAttribute), window.role == kAXScrollAreaRole,
              let focused = finder.element(kAXFocusedUIElementAttribute), focused.role == kAXGroupRole,
              focused.element(kAXParentAttribute) == window,
              let selected: [AXUIElement] = focused.value(kAXSelectedChildrenAttribute), !selected.isEmpty,
              NSWorkspace.shared.frontmostApplication == app else { return }

        // Use Finder's regular Move to Trash command, including its Undo support.
        event.setIntegerValueField(.keyboardEventKeycode, value: Int64(kVK_Delete))
        event.flags.remove(.maskSecondaryFn)
        event.flags.insert(.maskCommand)
        record(.desktopDeleteShortcut, .complete, started: started)
    }

    private func hit(at point: CGPoint) -> AXUIElement? {
        var result: AXUIElement?
        guard AXUIElementCopyElementAtPosition(system, Float(point.x), Float(point.y), &result) == .success else { return nil }
        return result
    }

    private func target(_ element: AXUIElement, at point: CGPoint, clicks: Int,
                        frontmost: NSRunningApplication?) -> Target? {
        if selection.contains(.dock), let app = dockApp(element) {
            let pid = app.processIdentifier
            let pending = dockMinimized[pid]
            if let pending {
                let minimized = pending.minimized
                if minimized == true { return .dock(app, .restore(pending)) }
                dockMinimized[pid] = nil
                pruneObservers()
                // The Dock can restore the window itself. If it is now the
                // focused window, this click can minimize it again.
                guard minimized == false else { return nil }
            }
            guard frontmost == app, let window = focusedWindow(pid), pending == nil || pending == window,
                  window.ordinaryWindow,
                  window.minimized == false, window.settable(kAXMinimizedAttribute) else { return nil }
            return .dock(app, .minimize(window))
        }
        guard selection.contains(.titleBar), (1...2).contains(clicks),
              let window = titleBar(element, at: point) else { return nil }
        return .title(window, clicks)
    }

    private func finishPress(_ event: CGEvent) {
        guard let press else { return }
        self.press = nil
        let point = event.location
        guard event.timestamp >= press.timestamp, event.timestamp - press.timestamp < 600_000_000,
              abs(point.x - press.point.x) < 5, abs(point.y - press.point.y) < 5, let element = hit(at: point) else {
            firstTitleClick = nil
            return
        }
        switch press.target {
        case let .title(window, count):
            guard titleBar(element, at: point) == window else { firstTitleClick = nil; return }
            if count == 1 { firstTitleClick = window; return }
            defer { firstTitleClick = nil }
            guard firstTitleClick == window else { return }
            DispatchQueue.main.async { [weak self] in self?.toggleFrame(window, at: point) }
        case let .dock(app, action):
            firstTitleClick = nil
            guard dockApp(element) == app else { return }
            // Dock keeps its native click. Give activation and reopening their
            // turn before deciding which exact window to manipulate.
            let revision = pointerRevision
            let deadline = ProcessInfo.processInfo.systemUptime + 0.75
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(80)) { [weak self] in
                self?.apply(action, to: app, revision: revision, deadline: deadline)
            }
        }
    }

    private func dockApp(_ element: AXUIElement) -> NSRunningApplication? {
        guard let pid = element.processID,
              NSRunningApplication(processIdentifier: pid)?.bundleIdentifier == "com.apple.dock",
              element.subrole == "AXApplicationDockItem",
              let url: URL = element.value(kAXURLAttribute), url.isFileURL else { return nil }
        let bundle = url.resolvingSymlinksInPath().standardizedFileURL
        return NSWorkspace.shared.runningApplications.first {
            !$0.isTerminated && $0.activationPolicy == .regular
                && $0.bundleURL?.resolvingSymlinksInPath().standardizedFileURL == bundle
        }
    }

    private func titleBar(_ element: AXUIElement, at point: CGPoint) -> AXUIElement? {
        let role = element.role
        guard role == kAXWindowRole || role == kAXToolbarRole || role == "AXTitleBar"
                || role == kAXStaticTextRole || role == kAXGroupRole else { return nil }
        guard let window = role == kAXWindowRole ? element : element.element(kAXWindowAttribute),
              let frame = window.frame, frame.contains(point),
              let close = window.element(kAXCloseButtonAttribute)?.frame,
              let zoom = window.element(kAXZoomButtonAttribute)?.frame,
              close.minY >= frame.minY, zoom.minY >= frame.minY else { return nil }
        // The system's traffic lights anchor the native title strip. Intersect
        // that strip with the AX hit result so tabs, text fields and buttons
        // never qualify just because they align with the title bar.
        if role == kAXGroupRole { return chromiumDragArea(element, in: window, frame: frame, at: point) ? window : nil }
        let controlTop = min(close.minY, zoom.minY)
        let bottom = max(close.maxY, zoom.maxY) + max(6, controlTop - frame.minY)
        guard bottom - frame.minY <= 64, point.y < bottom else { return nil }
        if role == kAXStaticTextRole, window.element("AXTitleUIElement") != element { return nil }
        return window
    }

    private func chromiumDragArea(_ element: AXUIElement, in window: AXUIElement, frame: CGRect, at point: CGPoint) -> Bool {
        guard let pid = window.processID,
              let id = NSRunningApplication(processIdentifier: pid)?.bundleIdentifier,
              ["com.google.Chrome", "org.chromium.Chromium"].contains(id),
              let bounds = element.frame, bounds.contains(point),
              abs(bounds.minY - frame.minY) < 2, abs(bounds.maxX - frame.maxX) < 2,
              (24...60).contains(bounds.height), bounds.minX > frame.minX + 80,
              let container = element.element(kAXParentAttribute), container.role == kAXGroupRole,
              let strip = container.frame, strip.contains(bounds), strip.height == bounds.height,
              strip.minX < bounds.minX, let siblings: [AXUIElement] = container.value(kAXChildrenAttribute),
              siblings.contains(where: { $0 != element && $0.role == kAXButtonRole
                  && ($0.frame?.maxX ?? .infinity) <= bounds.minX }),
              let children: [AXUIElement] = element.value(kAXChildrenAttribute), children.isEmpty else { return false }
        // Chromium exposes the empty end of its tab strip as a leaf group.
        // Tab groups, tabs and the tab search control have different bounds or roles.
        return true
    }

    private func focusedWindow(_ pid: pid_t) -> AXUIElement? {
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(app, 0.08)
        return app.element(kAXFocusedWindowAttribute)
    }

    private func toggleFrame(_ window: AXUIElement, at point: CGPoint) {
        guard !recheckingPermissions, Preferences.selection.contains(.titleBar), AXIsProcessTrusted(), CGPreflightListenEventAccess(),
              Preferences.nativeTitleBarIsOff,
              window.ordinaryWindow, window.minimized == false, window.settable(kAXPositionAttribute),
              window.settable(kAXSizeAttribute), let current = window.frame else { return }
        let displays = GestureDisplay.current()
        guard let display = displays.first(where: { $0.frame.contains(point) }) else { return }
        let started = ProcessInfo.processInfo.systemUptime
        if let saved = maximized[window],
           current.nearlyEquals(saved.applied) || !displays.contains(where: { $0.id == saved.display }) {
            let original = displays.contains(where: { $0.id == saved.display }) ? saved.original : display.reachable(saved.original)
            restoreFrame(window, to: original, started: started)
            return
        }

        let usable = display.usable
        let reduced = usable.insetBy(dx: usable.width / 8, dy: usable.height / 8).integral
        if current.edgesAreNear(usable, within: Self.layoutTolerance) {
            restoreFrame(window, to: reduced, started: started, allowLayoutAdjustment: true)
            return
        }

        let adjusted = window.setFrame(usable)
        let actual = window.frame
        if let actual, !actual.nearlyEquals(current) {
            maximized[window] = RestorableFrame(original: current, applied: actual, display: display.id)
            observe(window)
            let fillsDisplay = adjusted && actual.edgesAreNear(usable, within: Self.layoutTolerance)
            record(.windowGestureMaximize, fillsDisplay ? .complete : .issues, started: started)
        } else {
            maximized[window] = nil
            pruneObservers()
            // Apps such as Terminal snap to a character grid. If a successful
            // resize leaves a filled window where it was, let it shrink.
            let visible = current.intersection(usable)
            if adjusted, actual != nil, visible.width >= usable.width * 0.9, visible.height >= usable.height * 0.9 {
                restoreFrame(window, to: reduced, started: started, allowLayoutAdjustment: true)
                return
            }
            record(.windowGestureMaximize, .unavailable, started: started)
        }
    }

    private func restoreFrame(_ window: AXUIElement, to frame: CGRect, started: TimeInterval,
                              allowLayoutAdjustment: Bool = false) {
        guard window.setFrame(frame), let actual = window.frame else {
            record(.windowGestureRestore, .issues, started: started)
            return
        }
        let restored = allowLayoutAdjustment ? actual.edgesAreNear(frame, within: Self.layoutTolerance)
                                            : actual.nearlyEquals(frame)
        if restored {
            maximized[window] = nil
            pruneObservers()
        }
        record(.windowGestureRestore, restored ? .complete : .issues, started: started)
    }

    private func apply(_ action: DockClick, to app: NSRunningApplication, revision: UInt64, deadline: TimeInterval) {
        guard !recheckingPermissions, Preferences.selection.contains(.dock), AXIsProcessTrusted(),
              CGPreflightListenEventAccess(), !app.isTerminated else { return }
        if NSWorkspace.shared.frontmostApplication?.processIdentifier != app.processIdentifier {
            // An app in another Space may not be active by the time the Dock
            // animation starts. Any later mouse action takes precedence.
            guard case .restore = action, revision == pointerRevision, ProcessInfo.processInfo.systemUptime < deadline else { return }
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(50)) { [weak self] in
                self?.apply(action, to: app, revision: revision, deadline: deadline)
            }
            return
        }
        let pid = app.processIdentifier
        let started = ProcessInfo.processInfo.systemUptime
        switch action {
        case let .minimize(window):
            // A rapid following click can have arrived while the first action
            // was still waiting for Dock. It still toggles the same window.
            if dockMinimized[pid] == window {
                restoreDockWindow(window, pid: pid, started: started)
                return
            }
            guard focusedWindow(pid) == window, window.ordinaryWindow, window.minimized == false,
                  window.set(kAXMinimizedAttribute, kCFBooleanTrue) else { return }
            dockMinimized[pid] = window
            observe(window)
            record(.windowGestureDockMinimize, .complete, started: started)
        case let .restore(window):
            guard dockMinimized[pid] == window else { return }
            restoreDockWindow(window, pid: pid, started: started)
        }
    }

    private func restoreDockWindow(_ window: AXUIElement, pid: pid_t, started: TimeInterval) {
        guard let minimized = window.minimized else { return }
        if let current = focusedWindow(pid), current != window, !current.ordinaryWindow { return }
        guard !minimized || window.set(kAXMinimizedAttribute, kCFBooleanFalse) else {
            record(.windowGestureDockRestore, .unavailable, started: started)
            return
        }
        let raised = window.perform(kAXRaiseAction)
        _ = window.set(kAXMainAttribute, kCFBooleanTrue)
        dockMinimized[pid] = nil
        pruneObservers()
        record(.windowGestureDockRestore, raised ? .complete : .issues, started: started)
    }

    private func observe(_ window: AXUIElement) {
        guard let pid = window.processID else { return }
        let observer: AXObserver
        if let existing = observers[pid] {
            observer = existing
        } else {
            var new: AXObserver?
            let result = AXObserverCreate(pid, { _, window, _, context in
                guard let context, window.minimized == nil else { return }
                let controller = Unmanaged<GestureController>.fromOpaque(context).takeUnretainedValue()
                controller.maximized[window] = nil
                controller.tiled[window] = nil
                if let pid = window.processID, controller.dockMinimized[pid] == window {
                    controller.dockMinimized[pid] = nil
                }
                controller.pruneObservers()
            }, &new)
            guard result == .success, let new else { return }
            observer = new
            observers[pid] = new
            CFRunLoopAddSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(new), .commonModes)
        }
        AXObserverAddNotification(observer, window, kAXUIElementDestroyedNotification as CFString,
                                  Unmanaged.passUnretained(self).toOpaque())
    }

    private func forget(process pid: pid_t) {
        dockMinimized[pid] = nil
        maximized = maximized.filter { $0.key.processID != pid }
        tiled = tiled.filter { $0.key.processID != pid }
        pruneObservers()
    }

    private func pruneObservers() {
        let live = Set(maximized.keys.compactMap(\.processID))
            .union(tiled.keys.compactMap(\.processID)).union(dockMinimized.keys)
        for pid in Set(observers.keys).subtracting(live) {
            guard let observer = observers.removeValue(forKey: pid) else { continue }
            CFRunLoopRemoveSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(observer), .commonModes)
        }
    }

    private func record(_ operation: RichFilesDiagnostics.Operation, _ outcome: RichFilesDiagnostics.Outcome,
                        started: TimeInterval = ProcessInfo.processInfo.systemUptime) {
        RichFilesDiagnostics.record(operation, id: UUID(), outcome: outcome, started: started,
                                    total: 1, confirmed: outcome == .complete ? 1 : 0)
    }
}

private extension CGRect {
    func edgesAreNear(_ other: CGRect, within tolerance: CGFloat) -> Bool {
        abs(minX - other.minX) <= tolerance && abs(minY - other.minY) <= tolerance
            && abs(maxX - other.maxX) <= tolerance && abs(maxY - other.maxY) <= tolerance
    }

    func nearlyEquals(_ other: CGRect) -> Bool {
        abs(minX - other.minX) < 2 && abs(minY - other.minY) < 2
            && abs(width - other.width) < 2 && abs(height - other.height) < 2
    }
}
