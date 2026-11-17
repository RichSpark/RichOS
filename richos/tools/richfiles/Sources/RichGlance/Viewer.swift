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
    private let canvas = Canvas()
    private var folder: [URL] = []
    private var folderRequest: Request?
    private var pendingStep = 0
    private var displayRequest: Request?
    private var decodeWork: DispatchWorkItem?
    private var folderWork: DispatchWorkItem?
    private let decoder = DispatchQueue(label: "app.richos.richglance.decode", qos: .userInitiated)
    private let scanner = DispatchQueue(label: "app.richos.richglance.folder", qos: .userInitiated)

    init() {
        let area = NSScreen.main?.visibleFrame.size ?? CGSize(width: 1_200, height: 800)
        let size = CGSize(width: min(920, area.width * 0.82), height: min(620, area.height * 0.82))
        window = NSWindow(contentRect: CGRect(origin: .zero, size: size),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable],
                          backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.collectionBehavior.insert(.fullScreenPrimary)
        window.contentMinSize = CGSize(width: 300, height: 180)
        window.tabbingMode = .disallowed
        window.contentView = canvas
        window.center()
        canvas.step = { [weak self] in self?.step($0) }
        canvas.close = { [weak self] in self?.window.performClose(nil) }
    }

    func show() {
        if window.isMiniaturized { window.deminiaturize(nil) }
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(canvas)
    }

    func open(_ file: URL) {
        let request = Request()
        stopFolderScan()
        folder = [file]
        pendingStep = 0
        load(file, request: request)
        folderRequest = request
        let work = DispatchWorkItem { [weak self] in
            let images = try? GlanceFiles.folder(containing: file)
            DispatchQueue.main.async { [weak self] in
                guard let self, folderRequest?.id == request.id else { return }
                folderRequest = nil
                folderWork = nil
                folder = images ?? [file]
                request.record(.glanceFolder, images == nil ? .unavailable : .complete, total: folder.count)
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
        if folderRequest != nil {
            pendingStep += direction
            return
        }
        guard let file, let index = folder.firstIndex(where: { $0.path == file.path }), folder.count > 1 else { return }
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
