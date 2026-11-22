import AppKit
import RichFilesCore
import UniformTypeIdentifiers

@MainActor
final class CaptureController: NSObject, NSWindowDelegate {
    private struct Display {
        let id: CGDirectDisplayID
        let frame: CGRect
        let scale: CGFloat

        init?(_ screen: NSScreen) {
            guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else { return nil }
            id = number.uint32Value
            frame = screen.frame
            scale = screen.backingScaleFactor
        }

        var identifier: String { ScreenshotRepeatTarget.displayIdentifier(id) }
        var size: CGSize { CGSize(width: (frame.width * scale).rounded(), height: (frame.height * scale).rounded()) }
        func matches(_ screen: NSScreen) -> Bool {
            guard let other = Display(screen) else { return false }
            return id == other.id && frame == other.frame && scale == other.scale
        }
    }

    private struct Surface {
        let display: Display
        let window: CaptureWindow
        let canvas: CaptureCanvas
    }
    private enum Shot {
        case region(Display, CGRect, CGImage)
        case display(Display, CGImage)
        case window(CaptureFrame.WindowTarget, Display)
    }
    private enum Route { case copy, editor, save, pin }

    private let id = UUID()
    private let began = ProcessInfo.processInfo.systemUptime
    private let refreshPermission: (_ session: CaptureController, _ originPID: pid_t?) -> Void
    private let finish: (_ copied: String?, _ display: CGRect?, _ retained: UUID?) -> Void
    private let store: ScreenshotStore
    private let preferredAction: ScreenshotStore.Action?
    private let pinImage: (ScreenshotDocument) -> Void
    private let openEditor: (ScreenshotDocument) -> Void
    let repeatsRegion: Bool
    let copiesText: Bool
    private var repeatSeed: ScreenshotRepeatTarget?
    private var repeatSize: CGSize?
    private weak var previousWindow: NSWindow?
    private let previousApp: NSRunningApplication?
    private var display: Display?
    private var acquisition: Task<Void, Never>?
    private var overlay: CaptureWindow?
    private var repeatFlash: NSPanel?
    private var surfaces: [Surface] = []
    private var mode = CaptureMode.region
    private var candidates: [CaptureFrame.WindowTarget] = []
    private var windowTarget: CaptureFrame.WindowTarget?
    private var locked = false
    private var countdown: CaptureCountdown?
    private var countdownTask: Task<Void, Never>?
    private var output: Task<Void, Never>?
    private var scrolling: ScrollingCapture?
    private var askingScrollAccess = false
    private var scrollingDocument: ScreenshotDocument?
    private var countdownKeys: LiveCaptureKeys?
    private var pointerPosition = CGPoint.zero
    private var sourceCursor = NSCursor.arrow
    private var delay = 0
    private var includesPointer = false
    private var action = ScreenshotStore.Action.copy
    private var canvas: CaptureCanvas?
    private var savePanel: NSSavePanel?
    private var permissionPanel: NSPanel?
    private var isFinished = false
    private var permissionRequested = false
    private var waitingForCaptureConsent = false
    private var resumedAfterCaptureConsent = false
    private var keyLoss: Task<Void, Never>?

    init(previousWindow: NSWindow?, previousApp: NSRunningApplication?, store: ScreenshotStore,
         preferredAction: ScreenshotStore.Action? = nil, repeatsRegion: Bool = false, copiesText: Bool = false, pinImage: @escaping (ScreenshotDocument) -> Void, openEditor: @escaping (ScreenshotDocument) -> Void,
         refreshPermission: @escaping (_ session: CaptureController, _ originPID: pid_t?) -> Void,
         finish: @escaping (_ copied: String?, _ display: CGRect?, _ retained: UUID?) -> Void) {
        self.previousWindow = previousWindow
        self.previousApp = previousApp
        self.store = store
        self.preferredAction = preferredAction
        self.openEditor = openEditor
        self.pinImage = pinImage
        self.repeatsRegion = repeatsRegion
        self.copiesText = copiesText
        self.refreshPermission = refreshPermission
        self.finish = finish
        super.init()
        NSWorkspace.shared.notificationCenter.addObserver(
            self, selector: #selector(applicationActivated(_:)),
            name: NSWorkspace.didActivateApplicationNotification, object: nil
        )
        NotificationCenter.default.addObserver(
            self, selector: #selector(displaysChanged),
            name: NSApplication.didChangeScreenParametersNotification, object: nil
        )
    }

    func start() {
        let point = NSEvent.mouseLocation
        pointerPosition = point
        sourceCursor = NSCursor.currentSystem ?? .arrow
        delay = copiesText ? 0 : store.delay
        includesPointer = !copiesText && store.includesPointer
        action = copiesText ? .copy : preferredAction ?? store.action
        guard let screen = NSScreen.screens.first(where: { $0.frame.contains(point) }) ?? NSScreen.main,
              let display = Display(screen) else {
            stop(.unavailable, operation: .glanceCaptureFrame)
            return
        }
        self.display = display
        if !CGPreflightScreenCaptureAccess() {
            let granted = !permissionRequested && CGRequestScreenCaptureAccess()
            permissionRequested = true
            if !granted {
                record(.glanceCaptureFrame, .unavailable, since: began)
                showPermissionHelp(on: screen)
                // Core Graphics can retain this process's old answer after the user grants access.
                refreshPermission(self, permissionOriginPID)
                return
            }
        }
        dismissPermissionHelp()
        acquisition?.cancel()
        let displays = NSScreen.screens.compactMap(Display.init)
        if repeatsRegion, repeatSeed == nil {
            beginRepeat(displays: displays)
            return
        }
        acquisition = Task { [weak self] in
            do {
                let image = try await CaptureFrame.read(displayID: display.id, size: display.size, excludingOwnWindows: true)
                var frames = [(display, image)]
                for other in displays where other.id != display.id {
                    if let frame = try? await CaptureFrame.read(displayID: other.id, size: other.size, excludingOwnWindows: true) {
                        frames.append((other, frame))
                    }
                }
                guard !Task.isCancelled, let self, !isFinished else { return }
                candidates = CaptureFrame.targets()
                show(frames, on: display, point: point)
            } catch {
                guard !Task.isCancelled, let self else { return }
                record(.glanceCaptureFrame, .issues, since: began)
                showCaptureError()
            }
        }
    }

    private func beginRepeat(displays: [Display]) {
        let last = store.lastRegion()
        let matching = last.flatMap { region in displays.first { $0.identifier == region.display } }
        let box = matching.flatMap { display in last?.crop(at: display.scale, within: display.size) }
        if let last, let matching, let box, matching.scale == last.scale {
            captureAgain(last, display: matching, crop: box)
            return
        }

        activate()
        let alert = NSAlert()
        alert.messageText = last == nil ? "Nothing to capture again yet" : "That area can’t be captured again"
        if let last, let matching, box != nil {
            alert.informativeText = "The display changed from \(last.scale.formatted())× to \(matching.scale.formatted())×."
            alert.addButton(withTitle: "Capture at \(matching.scale.formatted())×")
        } else {
            alert.informativeText = last == nil
                ? "Capture a region first. Capture Again takes another picture of those same pixels."
                : "The saved display or area is unavailable. Choose where to capture it now."
        }
        let rescaled = alert.buttons.count == 1
        alert.addButton(withTitle: last == nil ? "Capture Region…" : "Select Area Again")
        alert.addButton(withTitle: "Cancel").keyEquivalent = "\u{1b}"
        let response = alert.runModal()
        if rescaled, response == .alertFirstButtonReturn, let last, let matching, let box {
            captureAgain(last, display: matching, crop: box)
        } else if response == (rescaled ? .alertSecondButtonReturn : .alertFirstButtonReturn) {
            repeatSeed = last
            repeatSize = last?.pixels.size
            if last == nil { repeatsFirstCapture() }
            else { start() }
        } else { stop(.cancelled) }
    }

    private func repeatsFirstCapture() {
        repeatSeed = ScreenshotRepeatTarget(id: UUID(), display: "", pixels: .zero, scale: 1)
        start()
    }

    private func captureAgain(_ last: ScreenshotRepeatTarget, display: Display, crop: CGRect) {
        repeatSeed = last
        self.display = display
        let started = ProcessInfo.processInfo.systemUptime
        acquisition = Task { [weak self] in
            guard let self else { return }
            do {
                let frame = try await CaptureFrame.read(displayID: display.id, size: display.size,
                                                        pointer: includesPointer, excludingOwnWindows: true)
                guard !Task.isCancelled, !isFinished else { return }
                guard let cropped = frame.cropping(to: crop),
                      cropped.width == Int(crop.width), cropped.height == Int(crop.height) else {
                    throw CaptureFrame.Failure.displayUnavailable
                }
                var document = ScreenshotDocument(original: cropped, scale: display.scale)
                document.repeatTarget = ScreenshotRepeatTarget(id: document.id, display: display.identifier, pixels: crop, scale: display.scale)
                await flashRegion(crop, on: display)
                guard !Task.isCancelled, !isFinished else { return }
                publish(document, route: .editor, started: started)
            } catch {
                guard !Task.isCancelled, !isFinished else { return }
                repeatSize = last.pixels.size
                start()
            }
        }
    }

    private func flashRegion(_ crop: CGRect, on display: Display) async {
        guard !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion else { return }
        let frame = CGRect(x: display.frame.minX + crop.minX / display.scale,
                           y: display.frame.maxY - crop.maxY / display.scale,
                           width: crop.width / display.scale, height: crop.height / display.scale)
        let flash = NSPanel(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        flash.isOpaque = false
        flash.backgroundColor = .clear
        flash.hasShadow = false
        flash.ignoresMouseEvents = true
        flash.level = .floating
        if let view = flash.contentView {
            view.wantsLayer = true
            view.layer?.backgroundColor = NSColor.systemYellow.withAlphaComponent(0.10).cgColor
            view.layer?.borderColor = NSColor.systemYellow.cgColor
            view.layer?.borderWidth = 2
        }
        repeatFlash = flash
        flash.orderFrontRegardless()
        try? await Task.sleep(for: .milliseconds(130))
        flash.orderOut(nil)
        repeatFlash = nil
    }

    func cancel() { stop(.cancelled) }

    var canRevisitPermission: Bool { permissionPanel != nil }

    private var permissionOriginPID: pid_t? {
        previousWindow == nil ? previousApp?.processIdentifier : nil
    }

    /// A repeat capture command or Dock click brings the recovery controls back.
    func revisitPermission() {
        guard let permissionPanel else { return }
        if CGPreflightScreenCaptureAccess() {
            start()
        } else {
            permissionPanel.orderFrontRegardless()
            permissionPanel.makeKey()
            refreshPermission(self, permissionOriginPID)
        }
    }

    private static func isCaptureConsent(_ application: NSRunningApplication?) -> Bool {
        application?.bundleIdentifier == "com.apple.UserNotificationCenter"
    }

    @objc private func applicationActivated(_ notification: Notification) {
        guard !isFinished,
              let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { return }
        if let scrolling {
            scrolling.appActivated(app)
            return
        }
        if askingScrollAccess { return }
        if Self.isCaptureConsent(app) {
            if overlay != nil, savePanel == nil { waitForCaptureConsent() }
            return
        }
        if waitingForCaptureConsent {
            waitingForCaptureConsent = false
            guard app == .current || app == previousApp else {
                stop(.cancelled, restoreFocus: false)
                return
            }
            resumedAfterCaptureConsent = true
            start()
        } else if let permissionPanel, app == .current {
            if CGPreflightScreenCaptureAccess() { start() }
            else { permissionPanel.orderFrontRegardless() }
        }
    }

    private func waitForCaptureConsent() {
        guard !waitingForCaptureConsent else { return }
        guard !resumedAfterCaptureConsent, canvas == nil || canvas?.selection.phase == .aiming else {
            stop(.interrupted, restoreFocus: false)
            return
        }
        waitingForCaptureConsent = true
        keyLoss?.cancel()
        clearSurfaces()
    }

    private func show(_ frames: [(Display, CGImage)], on display: Display, point: CGPoint) {
        if Self.isCaptureConsent(NSWorkspace.shared.frontmostApplication) {
            waitForCaptureConsent()
            return
        }
        clearSurfaces()
        for (screen, image) in frames {
            let window = CaptureWindow(contentRect: screen.frame, styleMask: [.borderless], backing: .buffered, defer: false)
            window.setFrame(screen.frame, display: false)
            window.level = .screenSaver
            window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
            window.isReleasedWhenClosed = false
            window.hasShadow = false
            window.acceptsMouseMovedEvents = true
            window.delegate = self
            window.title = "RichGlance capture"
            let frame = CGRect(origin: .zero, size: screen.frame.size)
            let geometry = GlanceCaptureGeometry(frame: CGSize(width: image.width, height: image.height), view: frame.size)
            let pointer = geometry.sourcePoint(CGPoint(x: point.x - screen.frame.minX, y: screen.frame.maxY - point.y))
            let canvas = CaptureCanvas(image: image, frame: frame, pointer: pointer)
            if let repeatSize { canvas.prefillRepeat(repeatSize) }
            canvas.copiesText = copiesText
            window.canvas = canvas
            canvas.copy = { [weak self] in self?.deliver(.copy) }
            canvas.edit = { [weak self] in self?.deliver(.editor) }
            canvas.save = { [weak self] in self?.deliver(.save) }
            canvas.pin = { [weak self] in self?.deliver(.pin) }
            canvas.scroll = { [weak self] index in
                guard ScrollingCapture.Mode.allCases.indices.contains(index) else { return }
                self?.startScrolling(ScrollingCapture.Mode.allCases[index])
            }
            canvas.escape = { [weak self] in self?.escapePressed() }
            canvas.cancel = { [weak self] in self?.cancel() }
            canvas.changeMode = { [weak self] in self?.setMode($0) }
            canvas.changeDelay = { [weak self] delay in self?.setDelay(delay) }
            canvas.changePointer = { [weak self] pointer in self?.setPointer(pointer) }
            canvas.changeAction = { [weak self] action in self?.setAction(action) }
            canvas.aim = { [weak self, weak window] point in
                guard let window else { return }
                self?.aim(on: screen, point: window.convertPoint(toScreen: point))
            }
            canvas.chooseTarget = { [weak self, weak window] point in
                guard let self, let window else { return }
                if let point { aim(on: screen, point: window.convertPoint(toScreen: point)) }
                chooseTarget()
            }
            canvas.nextTarget = { [weak self] in self?.nextTarget() }
            let background = NSImageView(frame: frame)
            background.image = NSImage(cgImage: image, size: frame.size)
            background.imageScaling = .scaleAxesIndependently
            background.setAccessibilityElement(false)
            let content = NSView(frame: frame)
            content.addSubview(background)
            content.addSubview(canvas)
            window.contentView = content
            surfaces.append(Surface(display: screen, window: window, canvas: canvas))
        }
        selectDisplay(display)
        prepareOverlay()
        if NSApp.isActive {
            focusCapture()
        } else {
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.activates = true
            configuration.addsToRecentItems = false
            NSWorkspace.shared.openApplication(at: Bundle.main.bundleURL, configuration: configuration) { [weak self] _, _ in
                Task { @MainActor in self?.focusCapture() }
            }
        }
        record(.glanceCaptureFrame, .complete, since: began)
    }

    private func prepareOverlay() {
        refreshOverlay()
        activate()
        for surface in surfaces where surface.window !== overlay { surface.window.orderFrontRegardless() }
    }

    private func focusCapture() {
        guard let overlay, let canvas else { return }
        overlay.makeKeyAndOrderFront(nil)
        canvas.focusAction()
    }

    private func selectDisplay(_ target: Display) {
        guard let surface = surfaces.first(where: { $0.display.id == target.id }) else { return }
        display = target
        canvas = surface.canvas
        overlay = surface.window
    }

    private func clearSurfaces() {
        for surface in surfaces {
            surface.window.delegate = nil
            surface.window.orderOut(nil)
        }
        surfaces.removeAll()
        overlay = nil
        canvas = nil
    }

    private func setMode(_ target: CaptureMode) {
        guard target != mode, output == nil, countdown == nil else { return }
        _ = canvas?.resetSelection()
        mode = target
        locked = false
        windowTarget = nil
        if target == .window, let display { aim(on: display, point: NSEvent.mouseLocation) }
        refreshOverlay()
        focusCapture()
    }

    private func aim(on target: Display, point: CGPoint) {
        guard !locked, countdown == nil, mode != .region || canvas?.selection.phase == .aiming else { return }
        let changed = display?.id != target.id
        if changed { selectDisplay(target) }
        var update = changed
        if mode == .window {
            let desktop = CGPoint(x: point.x, y: primaryTop - point.y)
            let candidate = candidates.first { $0.frame.contains(desktop) }
            if candidate?.id != windowTarget?.id { windowTarget = candidate; update = true }
        }
        if update {
            refreshOverlay()
            if changed { focusCapture() }
        }
    }

    private var primaryTop: CGFloat { NSScreen.screens.first?.frame.maxY ?? 0 }

    private func chooseTarget() {
        guard !locked else { return }
        guard mode == .display || windowTarget != nil else {
            canvas?.showError("Move over a window or press Tab to choose one.")
            return
        }
        locked = true
        refreshOverlay()
        focusCapture()
    }

    private func nextTarget() {
        guard !locked else { return }
        if mode == .display, let index = surfaces.firstIndex(where: { $0.display.id == display?.id }) {
            selectDisplay(surfaces[(index + 1) % surfaces.count].display)
        } else if mode == .window, !candidates.isEmpty {
            let index = candidates.firstIndex { $0.id == windowTarget?.id } ?? -1
            let candidate = candidates[(index + 1) % candidates.count]
            windowTarget = candidate
            let center = CGPoint(x: candidate.frame.midX, y: primaryTop - candidate.frame.midY)
            if let other = surfaces.first(where: { $0.display.frame.contains(center) }) { selectDisplay(other.display) }
        }
        refreshOverlay()
        focusCapture()
    }

    private func refreshOverlay() {
        for surface in surfaces {
            let active = surface.window === overlay
            let local = windowTarget.map {
                CGRect(x: $0.frame.minX - surface.display.frame.minX,
                       y: $0.frame.minY - (primaryTop - surface.display.frame.maxY),
                       width: $0.frame.width, height: $0.frame.height)
            }
            let rect: CGRect?
            let size: CGSize
            switch mode {
            case .window:
                rect = local
                size = local.map { CGSize(width: $0.width * (display?.scale ?? 1), height: $0.height * (display?.scale ?? 1)) } ?? .zero
            case .display:
                rect = active ? surface.canvas.bounds : nil
                size = surface.display.size
            case .region:
                rect = nil
                size = .zero
            }
            surface.canvas.configure(mode: mode, active: active, delay: delay, pointer: includesPointer, action: action,
                                     origin: previousApp?.localizedName ?? "")
            let dimensions = "\(Int(size.width.rounded())) × \(Int(size.height.rounded())) px"
            surface.canvas.updateTarget(rect, description: dimensions, locked: locked)
        }
    }

    private func setDelay(_ value: Int) {
        do { try store.setDelay(value); delay = value; refreshOverlay(); focusCapture() }
        catch { canvas?.showError("Could not save the delay setting.") }
    }

    private func setPointer(_ value: Bool) {
        do { try store.setPointer(value); includesPointer = value; refreshOverlay(); focusCapture() }
        catch { canvas?.showError("Could not save the pointer setting.") }
    }

    private func setAction(_ value: ScreenshotStore.Action) {
        do { try store.setAction(value); action = value; refreshOverlay(); focusCapture() }
        catch { canvas?.showError("Could not save the action setting.") }
    }

    private func escapePressed() {
        if countdown != nil {
            cancelCountdown()
        } else if let savePanel {
            savePanel.cancel(nil)
        } else if output != nil {
            output?.cancel()
            output = nil
            focusCapture()
        } else if locked {
            locked = false
            refreshOverlay()
            focusCapture()
        } else if canvas?.resetSelection() != true {
            cancel()
        }
    }

    private func startScrolling(_ requested: ScrollingCapture.Mode) {
        guard let canvas, canvas.ready, let viewport = canvas.visibleCrop,
              let display, let screen = NSScreen.screens.first(where: display.matches),
              scrolling == nil, countdown == nil, output == nil, savePanel == nil else { return }
        let crop = viewport.integral.intersection(CGRect(x: 0, y: 0, width: canvas.image.width, height: canvas.image.height))
        guard crop.width >= 24, crop.height >= 24 else {
            canvas.showError("Choose a scrolling area at least 24 × 24 pixels.")
            return
        }
        let displayBounds = CGDisplayBounds(display.id)
        let anchor = CGPoint(x: displayBounds.minX + crop.midX * displayBounds.width / CGFloat(canvas.image.width),
                             y: displayBounds.minY + crop.midY * displayBounds.height / CGFloat(canvas.image.height))
        let expected = mode == .window ? windowTarget : nil
        if mode == .window, let expected, !expected.frame.contains(anchor) {
            canvas.showError("Choose a scrolling area on the selected window.")
            return
        }
        let sourcePID = expected?.pid ?? CaptureFrame.targets().first { $0.frame.contains(anchor) }?.pid
        guard let sourcePID, let owner = NSRunningApplication(processIdentifier: sourcePID),
              owner.activationPolicy == .regular else {
            canvas.showError("Select an area inside the app you want to scroll.")
            return
        }
        let geometry = GlanceCaptureGeometry(frame: CGSize(width: canvas.image.width, height: canvas.image.height),
                                             view: display.frame.size)
        let box = geometry.viewRect(crop)
        let region = CGRect(x: display.frame.minX + box.minX, y: display.frame.maxY - box.maxY,
                            width: box.width, height: box.height)

        askingScrollAccess = true
        defer { askingScrollAccess = false }
        var mode = requested
        var access = ScrollAccessPrompt()
        prompt: while mode != .manual, !CGPreflightPostEventAccess() {
            let alert = NSAlert()
            alert.messageText = "Allow automatic scrolling"
            alert.informativeText = access.message
            alert.addButton(withTitle: access.allowTitle)
            alert.addButton(withTitle: "Scroll Manually")
            alert.addButton(withTitle: "Cancel")
            let answer: ScrollAccessPrompt.Answer
            switch alert.runModal() {
            case .alertFirstButtonReturn: answer = .allow
            case .alertSecondButtonReturn: answer = .scrollManually
            default: answer = .cancel
            }
            guard !isFinished else { return }
            switch access.next(after: answer, mode: mode, isTrusted: { CGPreflightPostEventAccess() },
                               request: { CGRequestPostEventAccess() }) {
            case .scroll(let chosen):
                mode = chosen
                break prompt
            case .ask: continue
            case .cancel:
                focusCapture()
                return
            }
        }

        let session = ScrollingCapture(mode: mode, source: owner, anchor: anchor, displayID: display.id,
                                       displaySize: display.size, crop: crop, region: region,
                                       visible: screen.visibleFrame, id: id)
        session.copy = { [weak self] in self?.deliverScroll(.copy) }
        session.edit = { [weak self] in self?.deliverScroll(.editor) }
        session.save = { [weak self] in self?.deliverScroll(.save) }
        session.cancel = { [weak self] in self?.cancel() }
        session.retry = { [weak self] in self?.retryScrolling() }
        session.opensEditor = action == .editor
        scrolling = session
        keyLoss?.cancel()
        for surface in surfaces { surface.window.orderOut(nil) }
        if #available(macOS 14.0, *) { _ = owner.activate(from: .current, options: []) }
        else { _ = owner.activate(options: []) }
        acquisition = Task { [weak self, weak session] in
            guard let self, let session else { return }
            for _ in 0..<12 {
                if NSWorkspace.shared.frontmostApplication == owner { break }
                try? await Task.sleep(for: .milliseconds(50))
                guard !Task.isCancelled else { return }
            }
            guard scrolling === session else { return }
            let front = CaptureFrame.targets().first { $0.frame.contains(anchor) }
            guard front?.pid == sourcePID, expected == nil || front?.id == expected?.id else {
                retryScrolling()
                canvas.showError("Bring the window to the front to capture its scrolling area.")
                return
            }
            session.start()
        }
    }

    private func retryScrolling() {
        scrolling?.close()
        scrolling = nil
        scrollingDocument = nil
        prepareOverlay()
        focusCapture()
    }

    private func deliverScroll(_ route: Route) {
        guard let scrolling, let image = scrolling.result, let display, savePanel == nil else { return }
        let document = scrollingDocument ?? ScreenshotDocument(original: image, scale: display.scale, kind: .scrolling)
        scrollingDocument = document
        publish(document, route: route, started: ProcessInfo.processInfo.systemUptime)
    }

    private func shot() -> Shot? {
        guard let display, let canvas, canvas.ready else { return nil }
        switch mode {
        case .region:
            return canvas.selection.crop.map { .region(display, $0, canvas.image) }
        case .display:
            return .display(display, canvas.image)
        case .window:
            return windowTarget.map { .window($0, display) }
        }
    }

    private func deliver(_ route: Route) {
        guard !isFinished, savePanel == nil, output == nil, countdown == nil else { return }
        guard let shot = shot() else {
            canvas?.showError("Choose an area to capture.")
            return
        }
        let destination = repeatsRegion && repeatSeed?.display.isEmpty == false ? .editor : route
        if delay > 0 { beginCountdown(shot, route: destination) }
        else { acquireOutput(shot, route: destination, fresh: false) }
    }

    private func acquireOutput(_ shot: Shot, route: Route, fresh: Bool) {
        let started = ProcessInfo.processInfo.systemUptime
        output = Task { [weak self] in
            guard let self else { return }
            do {
                let document = try await read(shot, fresh: fresh)
                guard !Task.isCancelled, !isFinished else { return }
                if copiesText {
                    let lines = try await ScreenshotText.read(.init(image: document.original))
                    guard !Task.isCancelled, !isFinished else { return }
                    output = nil
                    if ScreenshotText.copy(lines) {
                        record(.glanceOCR, .complete, since: started)
                        stop(.complete, copied: "Copied \(lines.count) \(lines.count == 1 ? "line" : "lines") of text")
                    } else {
                        record(.glanceOCR, lines.isEmpty ? .unavailable : .issues, since: started)
                        canvas?.showError(lines.isEmpty ? "No text found in that area" : "Could not copy the recognized text")
                        focusCapture()
                    }
                } else {
                    output = nil
                    publish(document, route: route, started: started)
                }
            } catch {
                guard !Task.isCancelled, !isFinished else { return }
                output = nil
                if fresh { prepareOverlay(); focusCapture() }
                if copiesText {
                    record(.glanceOCR, .issues, since: started)
                    canvas?.showError("Could not read text in this area.")
                    focusCapture()
                } else {
                    outputFailed("Could not capture the selected target. Choose it again and try once more.", since: started)
                }
            }
        }
    }

    private func read(_ shot: Shot, fresh: Bool) async throws -> ScreenshotDocument {
        let source: CGImage
        let scale: CGFloat
        let kind: ScreenshotKind
        let crop: CGRect?
        let pointerOrigin: CGPoint
        var name: String?
        var regionDisplay: String?
        switch shot {
        case .region(let display, let box, let frozen):
            kind = .region
            source = fresh ? try await CaptureFrame.read(displayID: display.id, size: display.size, pointer: includesPointer,
                                                        excludingOwnWindows: true) : frozen
            scale = CGFloat(source.width) / display.frame.width
            crop = box
            regionDisplay = display.identifier
            pointerOrigin = CGPoint(x: display.frame.minX, y: display.frame.maxY)
        case .display(let display, let frozen):
            kind = .display
            source = fresh ? try await CaptureFrame.read(displayID: display.id, size: display.size, pointer: includesPointer,
                                                        excludingOwnWindows: true) : frozen
            scale = CGFloat(source.width) / display.frame.width
            crop = nil
            pointerOrigin = CGPoint(x: display.frame.minX, y: display.frame.maxY)
        case .window(let target, let display):
            kind = .window
            name = target.name
            (source, scale) = try await CaptureFrame.read(windowID: target.id, scale: display.scale, pointer: fresh && includesPointer)
            crop = nil
            pointerOrigin = CGPoint(x: target.frame.minX, y: primaryTop - target.frame.minY)
        }
        let image = !fresh && includesPointer
            ? drawPointer(on: source, at: CGPoint(x: (pointerPosition.x - pointerOrigin.x) * scale,
                                                y: (pointerOrigin.y - pointerPosition.y) * scale), scale: scale)
            : source
        if let crop {
            guard let cropped = image.cropping(to: crop), cropped.width == Int(crop.width),
                  cropped.height == Int(crop.height) else { throw CaptureFrame.Failure.displayUnavailable }
            var document = ScreenshotDocument(original: cropped, scale: scale, kind: kind)
            if let regionDisplay {
                document.repeatTarget = ScreenshotRepeatTarget(id: document.id, display: regionDisplay, pixels: crop, scale: scale)
            }
            document.screenRect = CGRect(x: pointerOrigin.x + crop.minX / scale, y: pointerOrigin.y - crop.maxY / scale,
                                         width: crop.width / scale, height: crop.height / scale)
            return document
        }
        var document = ScreenshotDocument(original: image, scale: scale, kind: kind, name: name)
        document.screenRect = CGRect(x: pointerOrigin.x, y: pointerOrigin.y - CGFloat(image.height) / scale,
                                     width: CGFloat(image.width) / scale, height: CGFloat(image.height) / scale)
        return document
    }

    private func drawPointer(on image: CGImage, at point: CGPoint, scale: CGFloat) -> CGImage {
        let cursor = sourceCursor.image
        let hot = sourceCursor.hotSpot
        let rect = CGRect(x: point.x - hot.x * scale, y: point.y - hot.y * scale,
                          width: cursor.size.width * scale, height: cursor.size.height * scale)
        guard rect.intersects(CGRect(x: 0, y: 0, width: image.width, height: image.height)),
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
                                      bytesPerRow: 0, space: space,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return image }
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
        cursor.draw(in: CGRect(x: rect.minX, y: CGFloat(image.height) - rect.maxY, width: rect.width, height: rect.height))
        NSGraphicsContext.restoreGraphicsState()
        return context.makeImage() ?? image
    }

    private func publish(_ document: ScreenshotDocument, route: Route, started: TimeInterval) {
        switch route {
        case .editor:
            retain(document)
            record(.glanceCaptureOutput, .complete, since: started)
            stop(.complete, restoreFocus: false, editorDocument: document)
        case .copy:
            guard let data = ScreenshotRenderer.png(document.original), ScreenshotRenderer.copy(data) else {
                if scrolling == nil { prepareOverlay() }
                outputFailed("The selected image could not be copied.", since: started)
                return
            }
            let retained = retain(document)
            record(.glanceCaptureOutput, .complete, since: started)
            stop(.complete, copied: "Copied \(document.original.width) × \(document.original.height) px", retained: retained)
        case .pin:
            retain(document)
            pinImage(document)
            record(.glanceCaptureOutput, .complete, since: started)
            stop(.complete)
        case .save:
            if scrolling == nil, overlay?.isVisible != true { prepareOverlay(); focusCapture() }
            save(document)
        }
    }

    @discardableResult
    private func retain(_ document: ScreenshotDocument) -> UUID? {
        let start = ProcessInfo.processInfo.systemUptime
        do {
            if let target = document.repeatTarget { try store.rememberRegion(target) }
            if try store.retain(document) { return document.id }
        } catch {
            record(.glanceCaptureOutput, .issues, since: start)
        }
        return nil
    }

    private func save(_ document: ScreenshotDocument) {
        guard let window = scrolling?.reviewWindow ?? overlay, savePanel == nil else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.png]
        let stamp = DateFormatter()
        stamp.dateFormat = "yyyy-MM-dd 'at' h.mm.ss a"
        panel.nameFieldStringValue = "RichGlance Capture \(stamp.string(from: Date())).png"
        panel.canCreateDirectories = true
        savePanel = panel
        panel.level = window.level
        panel.beginSheetModal(for: window) { [weak self, weak panel] outcome in
            guard let self, let panel, savePanel === panel, !isFinished else { return }
            savePanel = nil
            guard outcome == .OK, let url = panel.url else {
                if scrolling == nil { focusCapture() }
                return
            }
            let started = ProcessInfo.processInfo.systemUptime
            do {
                guard let data = ScreenshotRenderer.png(document.original) else { throw CocoaError(.fileWriteUnknown) }
                try data.write(to: url, options: .atomic)
                retain(document)
                record(.glanceCaptureOutput, .complete, since: started)
                stop(.complete)
            } catch {
                outputFailed("The PNG could not be saved. Choose a different location and try again.", since: started)
            }
        }
    }

    private func beginCountdown(_ shot: Shot, route: Route) {
        guard let display else { return }
        let panel = CaptureCountdown(display: display.frame)
        panel.cancel = { [weak self] in self?.cancelCountdown() }
        countdown = panel
        keyLoss?.cancel()
        for surface in surfaces { surface.window.orderOut(nil) }
        restoreOrigin()
        panel.show(seconds: delay)
        countdownKeys = LiveCaptureKeys(escape: { [weak self] in self?.cancelCountdown() })
        countdownTask = Task { [weak self, weak panel] in
            guard let self, let panel else { return }
            let started = ProcessInfo.processInfo.systemUptime
            for seconds in stride(from: delay, to: 0, by: -1) {
                panel.update(seconds: seconds)
                try? await Task.sleep(for: .seconds(1))
                guard !Task.isCancelled, countdown === panel else { return }
            }
            closeCountdown()
            record(.glanceCaptureFrame, .complete, since: started)
            acquireOutput(shot, route: route, fresh: true)
        }
    }

    private func closeCountdown() {
        countdownTask?.cancel()
        countdownTask = nil
        countdown?.window.orderOut(nil)
        countdown = nil
        countdownKeys = nil
    }

    private func cancelCountdown() {
        guard countdown != nil else { return }
        closeCountdown()
        prepareOverlay()
        focusCapture()
    }

    private func outputFailed(_ message: String, since started: TimeInterval) {
        record(.glanceCaptureOutput, .issues, since: started)
        if let scrolling { scrolling.showError(message) }
        else { canvas?.showError(message); focusCapture() }
    }

    private func showPermissionHelp(on screen: NSScreen) {
        if let permissionPanel {
            permissionPanel.orderFrontRegardless()
            return
        }

        let width: CGFloat = 440
        let inset: CGFloat = 20
        let message = NSTextField(wrappingLabelWithString: "Allow the app named by macOS under Screen & System Audio Recording, then try again here. RichGlance may need to reopen after the grant, even if macOS restarted RichOS. Captures stay on this Mac.")
        message.preferredMaxLayoutWidth = width - 2 * inset
        let settings = NSButton(title: "Open System Settings", target: self, action: #selector(openScreenRecordingSettings))
        let retry = NSButton(title: "Try Again", target: self, action: #selector(retryScreenRecording))
        let cancel = NSButton(title: "Not Now", target: self, action: #selector(cancelScreenRecording))
        cancel.keyEquivalent = "\u{1b}"
        let buttons = NSStackView(views: [cancel, retry, settings])
        let stack = NSStackView(views: [message, buttons])
        stack.orientation = .vertical
        stack.alignment = .trailing
        stack.spacing = 14
        stack.translatesAutoresizingMaskIntoConstraints = false
        message.widthAnchor.constraint(equalToConstant: width - 2 * inset).isActive = true
        let content = NSView()
        content.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: inset),
            stack.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -inset),
            stack.topAnchor.constraint(equalTo: content.topAnchor, constant: inset),
            stack.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -inset)
        ])

        let visible = screen.visibleFrame
        let frame = CGRect(x: visible.midX - width / 2, y: visible.minY + inset,
                           width: width, height: stack.fittingSize.height + 2 * inset)
        let panel = NSPanel(contentRect: frame, styleMask: [.titled, .closable, .nonactivatingPanel],
                            backing: .buffered, defer: false)
        panel.title = "RichGlance Needs Screen Recording Permission"
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.contentView = content
        panel.delegate = self
        permissionPanel = panel
        // The first system request can still be visible after CGRequest returns.
        // Keep recovery controls below the system dialog without taking its focus.
        panel.orderFrontRegardless()
    }

    private func dismissPermissionHelp() {
        permissionPanel?.delegate = nil
        permissionPanel?.close()
        permissionPanel = nil
    }

    @objc private func openScreenRecordingSettings() {
        guard let settings = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") else { return }
        NSWorkspace.shared.open(settings)
    }

    @objc private func retryScreenRecording() { revisitPermission() }

    @objc private func cancelScreenRecording() { stop(.unavailable) }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        guard sender === permissionPanel else { return true }
        cancelScreenRecording()
        return false
    }

    private func showCaptureError() {
        if !CGPreflightScreenCaptureAccess(), let screen = NSScreen.screens.first(where: { display?.matches($0) == true }) {
            showPermissionHelp(on: screen)
            return
        }
        activate()
        let alert = NSAlert()
        alert.messageText = "Could not capture this display"
        alert.informativeText = "Try the capture again with your pointer on the display you want to use."
        alert.addButton(withTitle: "OK")
        alert.runModal()
        stop(.issues)
    }

    @objc private func displaysChanged() {
        guard surfaces.contains(where: { surface in !NSScreen.screens.contains(where: surface.display.matches) }) ||
                (surfaces.isEmpty && display.map { target in !NSScreen.screens.contains(where: target.matches) } == true)
        else { return }
        stop(.interrupted)
    }

    func windowDidResignKey(_ notification: Notification) {
        guard notification.object as? NSWindow === overlay, savePanel == nil, countdown == nil,
              scrolling == nil, !askingScrollAccess else { return }
        keyLoss?.cancel()
        // Workspace activation can arrive just after AppKit reports loss of the key window.
        keyLoss = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled, let self, let overlay, !overlay.isKeyWindow, savePanel == nil, countdown == nil, output == nil, scrolling == nil, !askingScrollAccess else { return }
            if Self.isCaptureConsent(NSWorkspace.shared.frontmostApplication) { waitForCaptureConsent() }
            else { stop(.cancelled, restoreFocus: false) }
        }
    }

    private func activate() {
        if #available(macOS 14.0, *) { NSApp.activate() }
        else { NSApp.activate(ignoringOtherApps: true) }
    }

    private func restoreOrigin() {
        if let previousWindow, previousWindow.isVisible, !previousWindow.isMiniaturized {
            previousWindow.makeKeyAndOrderFront(nil)
        } else if let previousApp, !previousApp.isTerminated {
            if #available(macOS 14.0, *) { _ = previousApp.activate(from: .current, options: []) }
            else { _ = previousApp.activate(options: []) }
        }
    }

    private func stop(_ outcome: RichFilesDiagnostics.Outcome,
                      operation: RichFilesDiagnostics.Operation = .glanceCaptureSession,
                      restoreFocus: Bool = true, copied: String? = nil, retained: UUID? = nil,
                      editorDocument: ScreenshotDocument? = nil) {
        guard !isFinished else { return }
        isFinished = true
        let performedCapture = scrolling?.reviewWindow?.isVisible == true || countdown != nil
        acquisition?.cancel()
        repeatFlash?.orderOut(nil)
        repeatFlash = nil
        acquisition = nil
        keyLoss?.cancel()
        NotificationCenter.default.removeObserver(self)
        NSWorkspace.shared.notificationCenter.removeObserver(self)
        dismissPermissionHelp()
        savePanel?.cancel(nil)
        savePanel = nil
        closeCountdown()
        output?.cancel()
        output = nil
        scrolling?.close()
        scrolling = nil
        scrollingDocument = nil
        clearSurfaces()
        if restoreFocus, NSApp.isActive || performedCapture { restoreOrigin() }
        record(operation, outcome, since: began)
        if let editorDocument { openEditor(editorDocument) }
        finish(copied, display?.frame, retained)
    }

    private func record(_ operation: RichFilesDiagnostics.Operation, _ outcome: RichFilesDiagnostics.Outcome, since: TimeInterval) {
        RichFilesDiagnostics.record(operation, id: id, outcome: outcome, started: since,
                                    total: 1, confirmed: outcome == .complete ? 1 : 0)
    }
}

private final class CaptureWindow: NSWindow {
    weak var canvas: CaptureCanvas?
    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, attachedSheet == nil, canvas?.handleKey(event) == true { return }
        super.sendEvent(event)
    }
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

/// The Accessibility question before automatic scrolling. macOS answers a request at once, before
/// the user can turn access on in System Settings, so a refused request asks again with Try Again
/// instead of ending the capture. The selection is kept until the user scrolls or cancels.
struct ScrollAccessPrompt {
    enum Answer { case allow, scrollManually, cancel }
    enum Next: Equatable { case scroll(ScrollingCapture.Mode), ask, cancel }

    private(set) var requested = false

    var allowTitle: String { requested ? "Try Again" : "Request Access" }
    var message: String {
        requested
            ? "Turn on the app named by macOS under Privacy & Security > Accessibility in System Settings, then click Try Again. You can scroll manually right away."
            : "RichGlance needs Accessibility access to scroll another app. You can scroll manually right away."
    }

    mutating func next(after answer: Answer, mode: ScrollingCapture.Mode,
                       isTrusted: () -> Bool, request: () -> Bool) -> Next {
        switch answer {
        case .allow:
            let granted = requested ? isTrusted() : request()
            requested = true
            return granted ? .scroll(mode) : .ask
        case .scrollManually: return .scroll(.manual)
        case .cancel: return .cancel
        }
    }
}
