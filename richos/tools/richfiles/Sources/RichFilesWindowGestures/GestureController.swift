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
    private let desktopFolder = DesktopFolderClick()
    private var desktopMenuOrigin: CGPoint?
    private struct NewDesktopFolder {
        let finder: pid_t
        var foreground: NSRunningApplication?
        let chosenAt: Date
    }
    private var newDesktopFolder: NewDesktopFolder?
    private struct DesktopIconClick {
        let icon: AXUIElement
        let finder: pid_t
        let foreground: NSRunningApplication?
        let timestamp: UInt64
    }
    private var desktopIconClick: DesktopIconClick?
    private var desktopKeyRevision: UInt64 = 0
    private var moveToTrashMenuItem: AXUIElement?
    private lazy var finderTrashTitles: Set<String> = {
        var titles: Set<String> = ["Move to Trash", "Move to Bin"]
        guard let bundle = Bundle(path: "/System/Library/CoreServices/Finder.app") else { return titles }
        let languages = UserDefaults(suiteName: "com.apple.finder")?.stringArray(forKey: "AppleLanguages")
        let localizations = Bundle.preferredLocalizations(from: bundle.localizations, forPreferences: languages)
        for language in Set(localizations + bundle.preferredLocalizations) {
            guard let path = bundle.path(forResource: language, ofType: "lproj"), let localized = Bundle(path: path) else { continue }
            let key = "N153"
            let title = localized.localizedString(forKey: key, value: nil, table: "LocalizableMerged")
            if title != key { titles.insert(title) }
        }
        return titles
    }()
    private var press: Press?
    private var pointerRevision: UInt64 = 0
    private var firstTitleClick: AXUIElement?
    private var maximized: [AXUIElement: RestorableFrame] = [:]
    private var tiled: [AXUIElement: RestorableFrame] = [:]
    private var tileDrag: TileDrag?
    private var switcher: WindowSwitcher?
    private var dockMinimized: [pid_t: AXUIElement] = [:]
    private var observers: [pid_t: AXObserver] = [:]
    private var appTermination: NSObjectProtocol?
    private var appDeactivation: NSObjectProtocol?
    private var accessibilityChanges: NSObjectProtocol?
    private var recheckingPermissions = false
    private var permissionCheckQueued = false
    private var permission: GesturePermission?
    private var permissionRestartFailed = false

    init() {
        AXUIElementSetMessagingTimeout(system, 0.08)
        appTermination = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didTerminateApplicationNotification, object: nil, queue: .main
        ) { [weak self] notification in
            guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { return }
            self?.forget(process: app.processIdentifier)
        }
        appDeactivation = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didDeactivateApplicationNotification, object: nil, queue: .main
        ) { [weak self] notification in
            guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
                  self?.newDesktopFolder?.foreground == app else { return }
            self?.newDesktopFolder?.foreground = nil
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

    var canRestartAfterUpdate: Bool { press == nil && tileDrag == nil && switcher == nil && !desktopFolder.busy }

    func refresh() {
        guard !recheckingPermissions else { return }
        let previous = selection
        selection = Preferences.selection
        if !selection.contains(.titleBar) && previous.contains(.titleBar) { maximized.removeAll() }
        if !selection.contains(.dock) && previous.contains(.dock) { dockMinimized.removeAll() }
        if !selection.contains(.tiling) && previous.contains(.tiling) { tiled.removeAll() }
        if !selection.contains(.switcher) && previous.contains(.switcher) { switcher?.dismiss() }
        if selection != previous {
            stopTap(keepingWindows: true)
            pruneObservers()
        }

        let status: Preferences.Status
        if selection.isEmpty {
            status = .off
        } else if permissionRestartFailed {
            status = .inputUnavailable
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
        permissionRestartFailed = false
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
        // A new process must obtain its own Accessibility trust decision.
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard !Preferences.restartHelper() else { return }
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.permissionCheckQueued = false
                self.recheckingPermissions = false
                self.permissionRestartFailed = true
                self.refresh()
            }
        }
    }

    private func startTap() -> Bool {
        if let tap { return CGEvent.tapIsEnabled(tap: tap) }
        var types: [CGEventType] = []
        if !selection.intersection([.titleBar, .dock, .tiling, .desktopFolder]).isEmpty {
            types = [.leftMouseDown, .leftMouseUp, .leftMouseDragged, .rightMouseDown, .otherMouseDown]
        }
        if selection.contains(.desktopDelete) {
            types += [.leftMouseDown, .leftMouseUp, .leftMouseDragged, .rightMouseDown, .rightMouseUp, .otherMouseDown]
        }
        if !selection.intersection([.desktopDelete, .tiling, .switcher]).isEmpty { types.append(.keyDown) }
        let mask = types.reduce(CGEventMask(0)) { $0 | (1 << $1.rawValue) }
        // Observe input before Finder and the Dock can act on a desktop gesture.
        guard let tap = CGEvent.tapCreate(tap: .cgSessionEventTap, place: .headInsertEventTap,
                                         options: .defaultTap, eventsOfInterest: mask, callback: { _, type, event, context in
            if let context {
                let controller = Unmanaged<GestureController>.fromOpaque(context).takeUnretainedValue()
                if type == .keyDown {
                    controller.desktopMenuOrigin = nil
                    controller.desktopKeyRevision &+= 1
                    if controller.handleWindowShortcut(event) {
                        controller.newDesktopFolder = nil
                        controller.desktopIconClick = nil
                        return nil
                    }
                    return controller.desktopKeyEvent(event)
                } else {
                    controller.receive(type, event)
                }
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
        desktopMenuOrigin = nil
        newDesktopFolder = nil
        desktopIconClick = nil
        desktopKeyRevision &+= 1
        self.source = nil
        tap = nil
        desktopFolder.reset()
        press = nil
        firstTitleClick = nil
        tileDrag = nil
        if keepingWindows { return }
        switcher?.dismiss()
        maximized.removeAll()
        tiled.removeAll()
        dockMinimized.removeAll()
        pruneObservers()
    }

    private func receive(_ type: CGEventType, _ event: CGEvent) {
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            pointerRevision &+= 1
            desktopFolder.reset()
            press = nil
            firstTitleClick = nil
            tileDrag = nil
            desktopMenuOrigin = nil
            newDesktopFolder = nil
            desktopIconClick = nil
            if let tap, AXIsProcessTrusted() { CGEvent.tapEnable(tap: tap, enable: true) }
            return
        }
        if type == .leftMouseDown || type == .rightMouseDown || type == .otherMouseDown || type == .leftMouseDragged {
            desktopIconClick = nil
        }
        if selection.contains(.desktopDelete) { trackDesktopMenu(type, event) }
        let modifiers: CGEventFlags = [.maskCommand, .maskAlternate, .maskControl, .maskShift, .maskSecondaryFn]
        guard event.flags.intersection(modifiers).isEmpty else {
            pointerRevision &+= 1
            desktopFolder.reset()
            press = nil
            firstTitleClick = nil
            tileDrag = nil
            return
        }
        switch type {
        case .leftMouseDown:
            pointerRevision &+= 1
            let desktopClick = !selection.intersection(.desktop).isEmpty
                && DesktopFolderClick.isDesktopWindow(at: event.unflippedLocation)
            let openFolder = desktopClick && selection.contains(.desktopFolder)
            let windowGestures = !selection.intersection([.titleBar, .dock, .tiling]).isEmpty
            if !openFolder { desktopFolder.reset() }
            guard desktopClick || windowGestures else { return }
            // Capture activation and window identity during mouse-down, before
            // native Dock mouse-up can activate, reopen or focus another window.
            let frontmost = NSWorkspace.shared.frontmostApplication
            let clicks = Int(event.getIntegerValueField(.mouseEventClickState))
            let element = hit(at: event.location)
            if desktopClick, selection.contains(.desktopDelete), clicks == 1,
               let icon = element, icon.role == kAXImageRole, let finder = icon.processID,
               NSRunningApplication(processIdentifier: finder)?.bundleIdentifier == "com.apple.finder" {
                desktopIconClick = DesktopIconClick(icon: icon, finder: finder, foreground: frontmost, timestamp: event.timestamp)
            }
            if openFolder { desktopFolder.down(event, element: element, clicks: clicks) }
            guard windowGestures else { return }
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
            desktopFolder.up(event)
            finishPress(event)
            finishTileDrag(at: event.location)
        case .leftMouseDragged:
            pointerRevision &+= 1
            desktopFolder.dragged(event)
            press = nil
            firstTitleClick = nil
            if let drag = tileDrag, hypot(event.location.x - drag.origin.x, event.location.y - drag.origin.y) >= 8 {
                tileDrag?.moved = true
            }
        case .rightMouseDown, .otherMouseDown:
            pointerRevision &+= 1
            desktopFolder.reset()
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
        guard pressed == [.maskCommand, .maskAlternate, .maskControl] else { return false }
        let key = Int(event.getIntegerValueField(.keyboardEventKeycode))
        let repeated = event.getIntegerValueField(.keyboardEventAutorepeat) != 0
        if key == kVK_ANSI_W, selection.contains(.switcher) {
            if !repeated {
                DispatchQueue.main.async { [weak self] in
                    guard let self, !self.recheckingPermissions, Preferences.selection.contains(.switcher),
                          AXIsProcessTrusted(), CGPreflightListenEventAccess() else { return }
                    if self.switcher == nil { self.switcher = WindowSwitcher() }
                    self.switcher?.toggle()
                }
            }
            return true
        }
        guard selection.contains(.tiling) else { return false }
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
        if !repeated {
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
        // On macOS 15 and later WindowServer may handle the same mouse-up.
        // Let it settle before deciding whether RichFiles needs to move the window.
        let delay: DispatchTimeInterval
        if #available(macOS 15, *) { delay = .milliseconds(350) }
        else { delay = .milliseconds(0) }
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
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
        let accepted: Bool
        let actual: CGRect?
        if #available(macOS 15, *), beforeDrag != nil,
           current.edgesAreNear(tile == .top ? display.usable : frame, within: Self.layoutTolerance) {
            // Native top-edge tiling fills the screen. Side and corner tiles can include margins.
            accepted = true
            actual = current
        } else {
            let moved = window.setFrame(frame)
            actual = window.frame
            accepted = moved && actual?.edgesAreNear(frame, within: Self.layoutTolerance) == true
        }
        if let actual, accepted || !actual.nearlyEquals(original) {
            tiled[window] = RestorableFrame(original: original, applied: actual, display: display.id)
            observe(window)
        }
        record(.windowGestureTile, accepted ? .complete : .issues, started: started)
    }

    private func restoreTile(_ window: AXUIElement, current: CGRect, on display: GestureDisplay) {
        let started = ProcessInfo.processInfo.systemUptime
        guard canTile(window), let saved = tiled[window], current.nearlyEquals(saved.applied) else {
            record(.windowGestureTileRestore, .unavailable, started: started)
            return
        }
        let displays = GestureDisplay.current()
        let originalDisplay = displays.first { $0.canReachTitleBar(of: saved.original) }
        let target = originalDisplay != nil ? saved.original : display.reachable(saved.original)
        let accepted = window.setFrame(target)
        let restored = accepted && window.frame?.edgesAreNear(target, within: Self.layoutTolerance) == true
        if restored {
            tiled[window] = nil
            pruneObservers()
        }
        record(.windowGestureTileRestore, restored ? .complete : .issues, started: started)
    }

    // Finder starts editing the name as soon as its New Folder menu command runs.
    // Remember that command only until the user interacts with the new editor.
    private func trackDesktopMenu(_ type: CGEventType, _ event: CGEvent) {
        let contextClick = type == .rightMouseDown || (type == .leftMouseDown && event.flags.contains(.maskControl))
        if contextClick {
            newDesktopFolder = nil
            desktopMenuOrigin = DesktopFolderClick.isDesktopWindow(at: event.unflippedLocation) ? event.location : nil
            return
        }
        switch type {
        case .leftMouseDown, .otherMouseDown:
            newDesktopFolder = nil
        case .leftMouseUp, .rightMouseUp:
            // A press-drag-release can choose the menu command in one gesture.
            guard let origin = desktopMenuOrigin, hypot(origin.x - event.location.x, origin.y - event.location.y) >= 5 else { return }
        default: return
        }
        guard desktopMenuOrigin != nil else { return }
        desktopMenuOrigin = nil
        guard type != .otherMouseDown, let item = hit(at: event.location), item.role == kAXMenuItemRole,
              item.value(kAXIdentifierAttribute) as String? == "cmdNewFolderAtMouseLocation:",
              let pid = item.processID, NSRunningApplication(processIdentifier: pid)?.bundleIdentifier == "com.apple.finder" else { return }
        newDesktopFolder = NewDesktopFolder(finder: pid, foreground: NSWorkspace.shared.frontmostApplication, chosenAt: Date())
    }

    private func desktopFileGroup(_ finder: AXUIElement) -> AXUIElement? {
        guard let window = finder.element(kAXFocusedWindowAttribute), window.role == kAXScrollAreaRole,
              let focused = finder.element(kAXFocusedUIElementAttribute), focused.role == kAXGroupRole,
              focused.element(kAXParentAttribute) == window else { return nil }
        return focused
    }

    private func desktopFileReferences(_ icons: [AXUIElement]) -> [NSURL] {
        icons.compactMap { icon in
            guard icon.role == kAXImageRole, let url: NSURL = icon.value(kAXURLAttribute),
                  CFURLIsFileReferenceURL(url as CFURL) else { return nil }
            return url
        }
    }

    // Finder can select the new Desktop item while another app still owns
    // the keyboard. This path must only act on the folder just created.
    private func backgroundFolderReference(_ command: NewDesktopFolder) -> NSURL? {
        let finder = finderElement(command.finder)
        guard let windows: [AXUIElement] = finder.value(kAXWindowsAttribute) else { return nil }
        for desktop in windows where desktop.role == kAXScrollAreaRole {
            guard let groups: [AXUIElement] = desktop.value(kAXChildrenAttribute) else { continue }
            for group in groups where group.role == kAXGroupRole {
                guard let icons: [AXUIElement] = group.value(kAXSelectedChildrenAttribute), icons.count == 1 else { continue }
                if let reference = desktopFileReferences(icons).first { return reference }
            }
        }
        return nil
    }

    private func recycleNewFolder(_ reference: NSURL, createdAfter: Date, started: TimeInterval) {
        // Keep the captured identity until the worker runs. Finder can expose
        // a stale Accessibility URL if a different item reused the same name.
        DispatchQueue.global(qos: .userInitiated).async {
            guard let url = reference.filePathURL,
                  let desktop = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask).first,
                  url.deletingLastPathComponent().resolvingSymlinksInPath().standardizedFileURL.path
                    == desktop.resolvingSymlinksInPath().standardizedFileURL.path,
                  let info = try? url.resourceValues(forKeys: [.creationDateKey, .isDirectoryKey, .isSymbolicLinkKey]),
                  info.isDirectory == true, info.isSymbolicLink == false,
                  let birth = info.creationDate, birth >= createdAfter,
                  let current = CFURLCreateFileReferenceURL(nil, url as CFURL, nil)?.takeRetainedValue(),
                  CFEqual(current, reference) else {
                self.record(.desktopDeleteShortcut, .unavailable, started: started)
                return
            }
            NSWorkspace.shared.recycle([url]) { moved, error in
                self.record(.desktopDeleteShortcut, error == nil && moved.count == 1 ? .complete : .issues, started: started)
            }
        }
    }

    private func finderTrashCommand(_ finder: AXUIElement) -> AXUIElement? {
        if let cached = moveToTrashMenuItem, cached.processID == finder.processID,
           let title: String = cached.value(kAXTitleAttribute), finderTrashTitles.contains(title) {
            return cached
        }
        guard let bar = finder.element(kAXMenuBarAttribute),
              let menus: [AXUIElement] = bar.value(kAXChildrenAttribute) else { return nil }
        // Match Finder's own localized command title, independent of a user's
        // keyboard layout or custom shortcut. Other Trash commands differ.
        for top in menus.dropFirst(2) {
            let sections: [AXUIElement] = top.value(kAXChildrenAttribute) ?? []
            for menu in sections where menu.role == kAXMenuRole {
                let items: [AXUIElement] = menu.value(kAXChildrenAttribute) ?? []
                if let item = items.first(where: {
                    guard let title: String = $0.value(kAXTitleAttribute) else { return false }
                    return self.finderTrashTitles.contains(title)
                }) {
                    moveToTrashMenuItem = item
                    return item
                }
            }
        }
        return nil
    }

    private func moveFinderSelectionToTrash(_ finder: AXUIElement, group: AXUIElement, icons: [AXUIElement],
                                            started: TimeInterval) {
        guard !icons.isEmpty else { return }
        guard let item = finderTrashCommand(finder), item.value(kAXEnabledAttribute) as Bool? == true,
              NSWorkspace.shared.frontmostApplication?.processIdentifier == finder.processID,
              desktopFileGroup(finder) == group,
              group.value(kAXSelectedChildrenAttribute) as [AXUIElement]? == icons,
              item.perform(kAXPressAction) else {
            record(.desktopDeleteShortcut, .unavailable, started: started)
            return
        }
        // AXPress only confirms delivery. Record success when Finder actually
        // removes the selected icons from the native Desktop view.
        confirmDesktopRemoval(group, icons: icons, started: started)
    }

    private func confirmDesktopRemoval(_ group: AXUIElement, icons: [AXUIElement], started: TimeInterval) {
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(100)) { [weak self] in
            guard let self else { return }
            let remaining: [AXUIElement]? = group.value(kAXChildrenAttribute)
            let removed = remaining.map { children in icons.filter { !children.contains($0) }.count } ?? 0
            if removed < icons.count, ProcessInfo.processInfo.systemUptime - started < 1 {
                self.confirmDesktopRemoval(group, icons: icons, started: started)
                return
            }
            RichFilesDiagnostics.record(.desktopDeleteShortcut, id: UUID(),
                                        outcome: removed == icons.count ? .complete : .issues,
                                        started: started, total: icons.count, confirmed: removed)
        }
    }

    private func desktopKeyEvent(_ event: CGEvent) -> Unmanaged<CGEvent>? {
        let click = desktopIconClick
        let newFolder = newDesktopFolder
        desktopIconClick = nil
        newDesktopFolder = nil
        let key = DesktopFileKey.classify(event)
        guard key != .passThrough, Preferences.selection.contains(.desktopDelete),
              let app = NSWorkspace.shared.frontmostApplication else { return Unmanaged.passUnretained(event) }

        switch key {
        case .passThrough:
            return Unmanaged.passUnretained(event)
        case .backspace:
            guard app.bundleIdentifier == "com.apple.finder",
                  desktopFileGroup(finderElement(app.processIdentifier)) != nil,
                  NSWorkspace.shared.frontmostApplication == app else { return Unmanaged.passUnretained(event) }
            return nil
        case .forwardDelete:
            let started = ProcessInfo.processInfo.systemUptime
            if let click, event.timestamp >= click.timestamp, event.timestamp - click.timestamp < 350_000_000,
               app.processIdentifier == click.finder || app == click.foreground {
                trashAfterDesktopClick(click, key: desktopKeyRevision, pointer: pointerRevision, started: started)
                return nil
            }
            if let newFolder, app.processIdentifier == newFolder.finder || newFolder.foreground == app {
                trashAfterNewFolder(newFolder, key: desktopKeyRevision, pointer: pointerRevision, started: started)
                return nil
            }
            if app.bundleIdentifier == "com.apple.finder" {
                let finder = finderElement(app.processIdentifier)
                if let group = desktopFileGroup(finder),
                   let icons: [AXUIElement] = group.value(kAXSelectedChildrenAttribute),
                   NSWorkspace.shared.frontmostApplication == app {
                    let key = desktopKeyRevision
                    let pointer = pointerRevision
                    DispatchQueue.main.async { [weak self] in
                        guard let self, self.desktopKeyRevision == key, self.pointerRevision == pointer,
                              Preferences.selection.contains(.desktopDelete) else { return }
                        self.moveFinderSelectionToTrash(finder, group: group, icons: icons, started: started)
                    }
                    return nil
                }
            }
            return Unmanaged.passUnretained(event)
        }
    }

    private func trashAfterNewFolder(_ folder: NewDesktopFolder, key: UInt64, pointer: UInt64, started: TimeInterval) {
        // The New Folder command can still be creating and selecting its
        // target when the key arrives. Wait without using the old selection.
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(15)) { [weak self] in
            guard let self, self.desktopKeyRevision == key, self.pointerRevision == pointer,
                  Preferences.selection.contains(.desktopDelete),
                  let app = NSWorkspace.shared.frontmostApplication,
                  app.processIdentifier == folder.finder || app == folder.foreground else { return }
            if let reference = self.backgroundFolderReference(folder), NSWorkspace.shared.frontmostApplication == app {
                self.recycleNewFolder(reference, createdAfter: folder.chosenAt, started: started)
            } else if ProcessInfo.processInfo.systemUptime - started < 0.5 {
                self.trashAfterNewFolder(folder, key: key, pointer: pointer, started: started)
            } else {
                self.record(.desktopDeleteShortcut, .unavailable, started: started)
            }
        }
    }

    private func finderElement(_ pid: pid_t) -> AXUIElement {
        let finder = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(finder, 0.08)
        return finder
    }

    private func trashAfterDesktopClick(_ click: DesktopIconClick, key: UInt64, pointer: UInt64, started: TimeInterval) {
        // The click is delivered after the tap returns. Wait for that exact
        // icon to become selected, and abandon the request on any newer input.
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(15)) { [weak self] in
            guard let self, self.desktopKeyRevision == key, self.pointerRevision == pointer,
                  Preferences.selection.contains(.desktopDelete) else { return }
            let app = NSWorkspace.shared.frontmostApplication
            let finder = self.finderElement(click.finder)
            if app?.processIdentifier == click.finder,
               let group = self.desktopFileGroup(finder),
               let icons: [AXUIElement] = group.value(kAXSelectedChildrenAttribute), icons == [click.icon],
               NSWorkspace.shared.frontmostApplication == app {
                self.moveFinderSelectionToTrash(finder, group: group, icons: icons, started: started)
            } else if ProcessInfo.processInfo.systemUptime - started < 0.25,
                      app?.processIdentifier == click.finder || app == click.foreground {
                self.trashAfterDesktopClick(click, key: key, pointer: pointer, started: started)
            } else {
                self.record(.desktopDeleteShortcut, .unavailable, started: started)
            }
        }
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
