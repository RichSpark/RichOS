import AppKit
import RichFilesCore

@MainActor
final class Viewer {
    private struct Request {
        let id = UUID()
        let started = ProcessInfo.processInfo.systemUptime

        func record(_ operation: RichFilesDiagnostics.Operation, _ outcome: RichFilesDiagnostics.Outcome,
                    started: TimeInterval? = nil, total: Int = 1, confirmed: Int? = nil) {
            RichFilesDiagnostics.record(operation, id: id, outcome: outcome, started: started ?? self.started,
                                        total: total, confirmed: confirmed ?? (outcome == .complete ? total : 0))
        }
    }

    let window: NSWindow
    private(set) var file: URL?
    private let canvas: Canvas
    var info: GlanceInfoView { canvas.info }
    private var folder: [URL]?
    private var folderRequest: Request?
    private var pendingStep = 0
    private var displayRequest: Request?
    private var decodeWork: DispatchWorkItem?
    private var playback: Task<Void, Never>?
    private var metadataWork: DispatchWorkItem?
    private var metadataRequest: (request: Request, began: TimeInterval)?
    private var folderWork: DispatchWorkItem?
    private let decoder = DispatchQueue(label: "app.richos.richglance.decode", qos: .userInitiated)
    private let scanner = DispatchQueue(label: "app.richos.richglance.folder", qos: .userInitiated)
    private let catalogReader = DispatchQueue(label: "app.richos.richglance.metadata", qos: .userInitiated)

    init(preferences: GlanceInfoPreferences) {
        canvas = Canvas(preferences: preferences)
        let area = NSScreen.main?.visibleFrame.size ?? CGSize(width: 1_200, height: 800)
        let size = CGSize(width: min(920, area.width * 0.82), height: min(620, area.height * 0.82))
        window = NSWindow(contentRect: CGRect(origin: .zero, size: size),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable],
                          backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.collectionBehavior.insert(.fullScreenPrimary)
        window.contentMinSize = CGSize(width: 300, height: 180)
        window.tabbingMode = .disallowed
        window.acceptsMouseMovedEvents = true
        window.autorecalculatesKeyViewLoop = false
        window.contentView = canvas
        window.center()
    }

    func handleKey(_ event: NSEvent) -> Bool {
        if info.handleKey(event) { return true }
        guard event.modifierFlags.intersection(GlanceInfoShortcut.modifiers).isEmpty else { return false }
        switch event.keyCode {
        case 53: window.performClose(nil)
        case 123: step(-1)
        case 124: step(1)
        default: return false
        }
        return true
    }

    func show() {
        if window.isMiniaturized { window.deminiaturize(nil) }
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(canvas)
    }

    func open(_ file: URL) {
        stopFolderScan()
        folder = nil
        pendingStep = 0
        load(file, request: Request())
    }

    private func scanFolder(from file: URL, request: Request) {
        folderRequest = request
        let work = DispatchWorkItem { [weak self] in
            let images = try? GlanceFiles.folder(containing: file)
            DispatchQueue.main.async { [weak self] in
                guard let self, folderRequest?.id == request.id else { return }
                folderRequest = nil
                folderWork = nil
                let items = images ?? [file]
                folder = items
                request.record(.glanceFolder, images == nil ? .unavailable : .complete, total: items.count)
                if images == nil { canvas.announce("Could not read this folder") }
                let offset = pendingStep
                pendingStep = 0
                if offset != 0 { step(offset) }
            }
        }
        folderWork = work
        scanner.async(execute: work)
    }

    func stop() {
        stopDisplay()
        stopFolderScan()
    }

    private func stopDisplay() {
        playback?.cancel()
        playback = nil
        metadataWork?.cancel()
        metadataWork = nil
        if let metadataRequest {
            metadataRequest.request.record(.glanceMetadata, .cancelled, started: metadataRequest.began)
        }
        metadataRequest = nil
        decodeWork?.cancel()
        decodeWork = nil
        displayRequest?.record(.glanceDisplay, .cancelled)
        displayRequest = nil
    }

    private func stopFolderScan() {
        folderWork?.cancel()
        folderWork = nil
        folderRequest?.record(.glanceFolder, .cancelled)
        folderRequest = nil
    }

    private func step(_ direction: Int) {
        guard let file else { return }
        guard let folder else {
            // Wait for a navigation gesture before asking macOS for folder access.
            // Opening one image does not require access to the rest of the Desktop.
            pendingStep += direction
            if folderRequest == nil { scanFolder(from: file, request: Request()) }
            return
        }
        guard let index = folder.firstIndex(where: { $0.path == file.path }), folder.count > 1 else { return }
        let next = (index + direction % folder.count + folder.count) % folder.count
        if next != index { load(folder[next], request: Request(), position: "\(next + 1) of \(folder.count)") }
    }

    private func load(_ file: URL, request: Request, position: String? = nil) {
        stopDisplay()
        displayRequest = request
        self.file = file
        window.title = file.lastPathComponent
        canvas.announce(nil)
        canvas.show(nil, filename: file.lastPathComponent)
        let work = DispatchWorkItem { [weak self] in
            let started = ProcessInfo.processInfo.systemUptime
            let decoded = Result { try GlanceImage.decode(file) }
            DispatchQueue.main.async { [weak self] in
                guard let self, displayRequest?.id == request.id else { return }
                decodeWork = nil
                displayRequest = nil
                let outcome: RichFilesDiagnostics.Outcome
                switch decoded {
                case .success(let image):
                    outcome = .complete
                    canvas.show(image, filename: file.lastPathComponent)
                    if let animation = image.animation { play(animation, request: request) }
                    readMetadata(for: file, request: request)
                case .failure(let error):
                    outcome = (error as? GlanceImage.Failure) == .unavailable ? .unavailable : .issues
                    canvas.show(nil, filename: file.lastPathComponent, message: "This image could not be opened.")
                }
                if let position { canvas.announce(position) }
                request.record(.glanceDecode, outcome, started: started)
                // Ends when the canvas accepts the result. The compositor presents it later.
                request.record(.glanceDisplay, outcome)
            }
        }
        decodeWork = work
        decoder.async(execute: work)
    }

    private func readMetadata(for file: URL, request: Request) {
        let began = ProcessInfo.processInfo.systemUptime
        metadataRequest = (request, began)
        let work = DispatchWorkItem { [weak self] in
            let result = Result { try GlanceMetadata.read(for: file) }
            DispatchQueue.main.async { [weak self] in
                guard let self, metadataRequest?.request.id == request.id else { return }
                metadataRequest = nil
                metadataWork = nil
                switch result {
                case .success(let metadata):
                    info.finishLookup(metadata, unavailable: false)
                    request.record(.glanceMetadata, .complete, started: began, total: metadata == nil ? 0 : 1)
                case .failure:
                    info.finishLookup(nil, unavailable: true)
                    request.record(.glanceMetadata, .unavailable, started: began)
                }
            }
        }
        metadataWork = work
        catalogReader.async(execute: work)
    }

    private func play(_ animation: GlanceAnimation, request: Request) {
        playback = Task { [weak self, decoder] in
            var index = 1
            var pass = 1
            var deadline = ProcessInfo.processInfo.systemUptime + animation.initialDuration
            while !Task.isCancelled {
                if index == animation.frameCount {
                    guard animation.loopCount == 0 || pass < animation.loopCount else { return }
                    if animation.loopCount > 0 { pass += 1 }
                    index = 0
                }
                // Decode at most one frame ahead, then wait out the current frame's duration.
                let next = index
                let began = ProcessInfo.processInfo.systemUptime
                let frame: (bitmap: CGImage, duration: TimeInterval)? = await withCheckedContinuation { continuation in
                    decoder.async { continuation.resume(returning: try? animation.frame(at: next)) }
                }
                guard !Task.isCancelled else { return }
                guard let frame else {
                    request.record(.glanceDecode, .issues, started: began)
                    return
                }
                do {
                    try await Task.sleep(for: .seconds(max(0, deadline - ProcessInfo.processInfo.systemUptime)))
                } catch { return }
                guard !Task.isCancelled, let self else { return }
                canvas.showAnimationFrame(frame.bitmap)
                deadline = ProcessInfo.processInfo.systemUptime + frame.duration
                index += 1
            }
        }
    }

    func openInPreview() {
        guard let file else { return }
        let request = Request()
        let workspace = NSWorkspace.shared
        guard let preview = workspace.urlForApplication(withBundleIdentifier: "com.apple.Preview") else {
            canvas.announce("Preview is unavailable")
            request.record(.glancePreview, .unavailable)
            return
        }
        Task { [weak self] in
            do {
                _ = try await workspace.open([file], withApplicationAt: preview, configuration: .init())
                request.record(.glancePreview, .complete)
            } catch {
                self?.canvas.announce("Could not open in Preview")
                request.record(.glancePreview, .issues)
            }
        }
    }
}
