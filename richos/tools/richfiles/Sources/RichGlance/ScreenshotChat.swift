import AppKit
import Darwin
import RichFilesCore

private enum ScreenshotChatLink {
    struct Chat: Decodable, Sendable {
        let id: String
        let title: String
        let current: Bool
    }

    struct Reply: Decodable, Sendable {
        let chats: [Chat]?
        let attached: Bool?
        let error: String?
    }

    struct Problem: LocalizedError {
        let errorDescription: String?
        init(_ message: String) { errorDescription = message }
    }

    static func list(_ pid: pid_t) async throws -> [Chat] {
        let result = try await exchange(pid, header: ["action": "list"])
        if let error = result.error { throw Problem(error) }
        return result.chats ?? []
    }

    static func attach(_ png: Data, preview: Data, thread: String?, pid: pid_t) async throws {
        var header: [String: any Sendable] = ["action": "attach", "length": png.count, "preview_length": preview.count]
        if let thread { header["thread"] = thread }
        let result = try await exchange(pid, header: header, png: png, preview: preview)
        guard result.attached == true else { throw Problem(result.error ?? "RichOS could not attach this screenshot.") }
    }

    private static func exchange(_ pid: pid_t, header: [String: any Sendable], png: Data? = nil, preview: Data? = nil) async throws -> Reply {
        // Keep this directory in sync with the RichOS listener. Darwin gives both apps
        // the same per-user directory when either was launched with a different HOME.
        let count = confstr(_CS_DARWIN_USER_TEMP_DIR, nil, 0)
        guard count > 1 else { throw Problem("Could not reach RichOS.") }
        var directory = [CChar](repeating: 0, count: count)
        guard confstr(_CS_DARWIN_USER_TEMP_DIR, &directory, count) == count else { throw Problem("Could not reach RichOS.") }
        let base = FileManager.default.string(withFileSystemRepresentation: directory, length: count - 1)
        let path = URL(fileURLWithPath: base, isDirectory: true)
            .appendingPathComponent("richglance-chat/\(pid).sock").path
        let data = try JSONSerialization.data(withJSONObject: header.compactMapValues { value in
            if let thread = value as? String { return thread as Any }
            if let number = value as? Int { return number as Any }
            return nil
        })
        return try await Task.detached(priority: .userInitiated) {
            let fd = socket(AF_UNIX, SOCK_STREAM, 0)
            guard fd >= 0 else { throw Problem("Could not reach RichOS.") }
            let socket = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
            defer { try? socket.close() }
            var ignorePipe: Int32 = 1
            var time = timeval(tv_sec: 65, tv_usec: 0)
            _ = setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &ignorePipe, socklen_t(MemoryLayout<Int32>.size))
            _ = setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &time, socklen_t(MemoryLayout<timeval>.size))
            time.tv_sec = 10
            _ = setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &time, socklen_t(MemoryLayout<timeval>.size))
            var address = sockaddr_un()
            address.sun_family = sa_family_t(AF_UNIX)
            address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
            let name = Array(path.utf8CString)
            guard name.count <= MemoryLayout.size(ofValue: address.sun_path) else { throw Problem("Could not reach RichOS.") }
            withUnsafeMutableBytes(of: &address.sun_path) { target in
                name.withUnsafeBytes { target.copyBytes(from: $0) }
            }
            let size = socklen_t(address.sun_len)
            let result = withUnsafePointer(to: &address) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    connect(fd, $0, size)
                }
            }
            guard result == 0 else { throw Problem("There is no chat to attach to.") }
            try socket.write(contentsOf: data)
            try socket.write(contentsOf: Data([10]))
            if let png { try socket.write(contentsOf: png) }
            if let preview { try socket.write(contentsOf: preview) }
            var bytes = Data()
            while bytes.count < 65_536 {
                guard let piece = try socket.read(upToCount: 4_096), !piece.isEmpty else { break }
                bytes.append(piece)
                if bytes.last == 10 { return try JSONDecoder().decode(Reply.self, from: bytes) }
            }
            throw Problem("RichOS did not respond to this attachment.")
        }.value
    }
}

@MainActor
final class ScreenshotChat: NSObject {
    private let window: ScreenshotEditorWindow
    private weak var parent: NSWindow?
    private let surface = ScreenshotSurface(.floating)
    private let heading = screenshotLabel("Attach to a RichOS chat", weight: .semibold)
    private let details = NSTextField(wrappingLabelWithString: "It goes into the chat’s message box as a draft. Nothing is sent until you send it in RichOS.")
    private let caption = screenshotLabel()
    private let thumb = NSImageView()
    private let scroller = NSScrollView()
    private let list = NSStackView()
    private let cancel = ScreenshotButton("Cancel")
    private let confirm = ScreenshotButton("Attach", style: .primary)
    private let launch = ScreenshotButton("Open RichOS")
    private let newChat = ScreenshotButton("Start a New Chat", style: .primary)
    private let image: CGImage
    private let name: String
    private var chats: [ScreenshotChatLink.Chat] = []
    private var choices: [ScreenshotButton] = []
    private var selection = 0
    private var pid: pid_t?
    private var busy = false
    private var noChat = false
    var closed: (() -> Void)?

    init(parent: NSWindow, image: CGImage, name: String) {
        self.parent = parent
        self.image = image
        self.name = name
        window = ScreenshotEditorWindow(contentRect: CGRect(x: 0, y: 0, width: 500, height: 475),
                                        styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        super.init()
        window.contentView = surface
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.setAccessibilityTitle("Attach to a RichOS chat")
        window.intercept = { [weak self] event in self?.handle(event) ?? false }
        window.isReleasedWhenClosed = false
        for view in [heading, details, caption, thumb, scroller, cancel, confirm, launch, newChat] {
            surface.addSubview(view)
        }
        heading.font = .systemFont(ofSize: 19, weight: .semibold)
        heading.frame = CGRect(x: 22, y: 24, width: 450, height: 28)
        thumb.frame = CGRect(x: 22, y: 64, width: 58, height: 53)
        thumb.image = NSImage(cgImage: image, size: .zero)
        thumb.imageScaling = .scaleProportionallyUpOrDown
        thumb.setAccessibilityElement(false)
        caption.stringValue = "\(name) · \(image.width) × \(image.height) px"
        caption.frame = CGRect(x: 92, y: 77, width: 382, height: 48)
        details.font = .systemFont(ofSize: 16)
        details.frame = CGRect(x: 22, y: 130, width: 456, height: 64)
        scroller.frame = CGRect(x: 22, y: 200, width: 456, height: 206)
        scroller.hasVerticalScroller = true
        scroller.drawsBackground = false
        scroller.autohidesScrollers = true
        list.orientation = .vertical
        list.alignment = .leading
        list.spacing = 5
        scroller.documentView = list
        cancel.frame = CGRect(x: 286, y: 421, width: 92, height: 35)
        confirm.frame = CGRect(x: 386, y: 421, width: 92, height: 35)
        launch.frame = CGRect(x: 22, y: 421, width: 128, height: 35)
        newChat.frame = CGRect(x: 158, y: 421, width: 176, height: 35)
        cancel.onPress = { [weak self] in self?.end() }
        confirm.onPress = { [weak self] in self?.attach() }
        launch.onPress = { [weak self] in self?.openRichOS(thenAttach: false) }
        newChat.onPress = { [weak self] in self?.openRichOS(thenAttach: true) }
        surface.changedAppearance = { [weak self] in self?.recolor() }
        recolor()
    }

    func show() {
        parent?.beginSheet(window)
        refresh()
    }

    private var richOS: NSRunningApplication? {
        NSRunningApplication.runningApplications(withBundleIdentifier: GlanceHelper.richOSID).first { !$0.isTerminated }
    }

    private func recolor() {
        let p = ScreenshotEditorPalette(surface)
        heading.textColor = p.ink
        caption.textColor = p.ink
        details.textColor = p.muted
    }

    private func refresh() {
        guard let richOS else { unavailable(); return }
        busy = true
        details.stringValue = "Finding chats in RichOS…"
        launch.isHidden = true
        newChat.isHidden = true
        confirm.isEnabled = false
        Task { [weak self] in
            let chats = try? await ScreenshotChatLink.list(richOS.processIdentifier)
            guard let self, window.sheetParent != nil else { return }
            busy = false
            if let chats {
                self.chats = chats
                pid = richOS.processIdentifier
                renderChoices()
            } else { unavailable() }
        }
    }

    private func unavailable(_ message: String = "There is no chat to attach to.") {
        noChat = true
        busy = false
        pid = nil
        details.stringValue = message
        confirm.isHidden = true
        scroller.isHidden = true
        launch.isHidden = false
        newChat.isHidden = false
        cancel.frame.origin.x = 386
        window.makeFirstResponder(cancel)
    }

    private func renderChoices() {
        noChat = false
        confirm.isHidden = false
        confirm.isEnabled = true
        launch.isHidden = true
        newChat.isHidden = true
        scroller.isHidden = false
        cancel.frame.origin.x = 286
        details.stringValue = "It goes into the chat’s message box as a draft. Nothing is sent until you send it in RichOS."
        for view in list.arrangedSubviews { list.removeArrangedSubview(view); view.removeFromSuperview() }
        choices = (chats.map { $0.title } + ["New chat"]).enumerated().map { index, title in
            let button = ScreenshotButton(title)
            button.toolTip = index < chats.count && chats[index].current ? "Open in RichOS now" : title
            button.onPress = { [weak self] in self?.select(index) }
            button.widthAnchor.constraint(equalToConstant: 436).isActive = true
            button.heightAnchor.constraint(equalToConstant: 37).isActive = true
            list.addArrangedSubview(button)
            return button
        }
        list.frame = CGRect(x: 0, y: 0, width: 436, height: list.fittingSize.height)
        select(chats.firstIndex(where: \.current) ?? 0)
    }

    private func select(_ index: Int) {
        guard choices.indices.contains(index) else { return }
        selection = index
        for (index, button) in choices.enumerated() { button.isSelected = selection == index }
        choices[index].scrollToVisible(choices[index].bounds)
        window.makeFirstResponder(choices[index])
    }

    private func attach() {
        guard !busy, let pid else { return }
        send(to: selection < chats.count ? chats[selection].id : nil, pid: pid)
    }

    private func send(to thread: String?, pid: pid_t) {
        guard let png = ScreenshotRenderer.png(image) else { unavailable("Could not prepare this screenshot."); return }
        busy = true
        confirm.isEnabled = false
        cancel.isEnabled = false
        launch.isEnabled = false
        newChat.isEnabled = false
        details.stringValue = "Opening the draft in RichOS…"
        let started = ProcessInfo.processInfo.systemUptime
        let preview = makePreview()
        Task { [weak self] in
            guard let self else { return }
            do {
                try await ScreenshotChatLink.attach(png, preview: preview, thread: thread, pid: pid)
                RichFilesDiagnostics.record(.glanceShare, id: UUID(), outcome: .complete, started: started, total: 1, confirmed: 1)
                end()
            } catch {
                RichFilesDiagnostics.record(.glanceShare, id: UUID(), outcome: .issues, started: started, total: 1, confirmed: 0)
                busy = false
                confirm.isEnabled = true
                cancel.isEnabled = true
                launch.isEnabled = true
                newChat.isEnabled = true
                details.stringValue = error.localizedDescription
                window.makeFirstResponder(cancel)
            }
        }
    }

    private func makePreview() -> Data {
        let scale = 84 / CGFloat(max(image.width, image.height))
        let width = max(1, Int(CGFloat(image.width) * scale))
        let height = max(1, Int(CGFloat(image.height) * scale))
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                                      bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return Data() }
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return context.makeImage().flatMap(ScreenshotRenderer.png) ?? Data()
    }

    private func openRichOS(thenAttach: Bool) {
        guard !busy, let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: GlanceHelper.richOSID) else {
            details.stringValue = "RichOS could not be found on this Mac."
            return
        }
        busy = true
        details.stringValue = "Opening RichOS…"
        Task { [weak self] in
            guard let self else { return }
            let config = NSWorkspace.OpenConfiguration()
            config.activates = true
            guard let app = try? await NSWorkspace.shared.openApplication(at: url, configuration: config) else {
                unavailable("RichOS could not be opened.")
                return
            }
            let deadline = ContinuousClock.now + .seconds(16)
            while window.sheetParent != nil, ContinuousClock.now < deadline {
                if let chats = try? await ScreenshotChatLink.list(app.processIdentifier) {
                    busy = false
                    pid = app.processIdentifier
                    if thenAttach { send(to: nil, pid: app.processIdentifier) }
                    else { self.chats = chats; renderChoices() }
                    return
                }
                try? await Task.sleep(for: .milliseconds(300))
            }
            unavailable("RichOS is still opening. Try again.")
        }
    }

    private func handle(_ event: NSEvent) -> Bool {
        if event.keyCode == 53, cancel.isEnabled { end(); return true }
        guard !busy, !noChat else { return false }
        switch event.keyCode {
        case 125: select(min(selection + 1, choices.count - 1))
        case 126: select(max(0, selection - 1))
        case 36, 76:
            if window.firstResponder === cancel { end() }
            else { attach() }
        default: return false
        }
        return true
    }

    private func end() {
        parent?.endSheet(window)
        window.orderOut(nil)
        closed?()
    }
}
