import AppKit
import RichFilesCore

@MainActor
final class ScrollingCapture: NSObject, NSWindowDelegate {
    enum Mode: String, CaseIterable {
        case manual, down, right, both

        var title: String {
            switch self {
            case .manual: return "Scroll Manually"
            case .down: return "Auto Scroll Down"
            case .right: return "Auto Scroll Right"
            case .both: return "Auto Scroll Down and Right"
            }
        }
    }

    private let mode: Mode
    private let source: NSRunningApplication
    private let anchor: CGPoint
    private let displayID: CGDirectDisplayID
    private let displaySize: CGSize
    private let crop: CGRect
    private let visible: CGRect
    private let guide: NSWindow
    private let panel: ScrollPanel
    private let assembler = ScrollAssembly()
    private let status = NSTextField(wrappingLabelWithString: "Preparing scrolling capture…")
    private let sizeLabel = NSTextField(labelWithString: "")
    private let thumbnail = NSImageView()
    private let stopButton = NSButton(title: "Stop", target: nil, action: nil)
    private var stream: ScrollFrameStream?
    private var frames: Task<Void, Never>?
    private var automatic: Task<Void, Never>?
    private var deadline: Task<Void, Never>?
    private var completing: Task<Void, Never>?
    private let started = ProcessInfo.processInfo.systemUptime
    private let id: UUID
    private var firstFrameMS: Int?
    private var received = 0
    private var accepted = 0
    private var unmatched = 0
    private var hasUnmatchedEnd = false
    private var lastPreview = 0.0
    private var reported = false
    private var recording = true

    private(set) var result: CGImage?
    var reviewWindow: NSWindow? { result == nil ? nil : panel }
    var copy: (() -> Void)?
    var save: (() -> Void)?
    var cancel: (() -> Void)?

    init(mode: Mode, source: NSRunningApplication, anchor: CGPoint, displayID: CGDirectDisplayID,
         displaySize: CGSize, crop: CGRect, region: CGRect, visible: CGRect, id: UUID) {
        self.mode = mode
        self.source = source
        self.anchor = anchor
        self.displayID = displayID
        self.displaySize = displaySize
        self.crop = crop
        self.visible = visible
        self.id = id
        guide = NSWindow(contentRect: region.insetBy(dx: -3, dy: -3), styleMask: .borderless,
                         backing: .buffered, defer: false)
        guide.backgroundColor = .clear
        guide.isOpaque = false
        guide.hasShadow = false
        guide.ignoresMouseEvents = true
        guide.level = .floating
        guide.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        guide.isReleasedWhenClosed = false
        guide.contentView = ScrollGuide()
        panel = ScrollPanel(contentRect: CGRect(x: 0, y: 0, width: 340, height: 105),
                            styleMask: [.titled, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.title = "Scrolling Capture"
        panel.level = .floating
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        super.init()

        panel.delegate = self
        panel.cancel = { [weak self] in self?.cancel?() }
        thumbnail.imageScaling = .scaleProportionallyUpOrDown
        thumbnail.setAccessibilityElement(false)
        thumbnail.widthAnchor.constraint(equalToConstant: 58).isActive = true
        thumbnail.heightAnchor.constraint(equalToConstant: 52).isActive = true
        let progress = NSStackView(views: [status, sizeLabel])
        progress.orientation = .vertical
        progress.alignment = .leading
        progress.spacing = 4
        status.preferredMaxLayoutWidth = 232
        sizeLabel.font = .monospacedDigitSystemFont(ofSize: 12, weight: .medium)
        let row = NSStackView(views: [thumbnail, progress])
        row.alignment = .centerY
        row.spacing = 10
        stopButton.target = self
        stopButton.action = #selector(stopPressed)
        stopButton.isEnabled = false
        stopButton.keyEquivalent = "\r"
        let buttons = NSStackView(views: [
            NSButton(title: "Cancel", target: self, action: #selector(cancelPressed)), stopButton
        ])
        install(rows: [row, buttons])
        let size = panel.frame.size
        let x = min(max(visible.minX + 12, region.midX - size.width / 2), visible.maxX - size.width - 12)
        let choices = [region.minY - size.height - 10, region.maxY + 10, visible.minY + 12]
        let y = choices.first { $0 >= visible.minY + 8 && $0 + size.height <= visible.maxY - 8 } ?? visible.minY
        panel.setFrameOrigin(CGPoint(x: x, y: y))
    }

    func start() {
        guide.orderFrontRegardless()
        panel.orderFrontRegardless()
        let windows = Set([guide, panel].compactMap { CGWindowID(exactly: $0.windowNumber) })
        frames = Task { [weak self] in
            guard let self else { return }
            do {
                let opened = try await ScrollFrameStream.open(displayID: displayID, size: displaySize,
                                                              crop: crop, excluding: windows)
                guard recording, !Task.isCancelled else { await opened.stop(); return }
                stream = opened
                for try await frame in opened.images {
                    guard recording, !Task.isCancelled else { break }
                    if let front = NSWorkspace.shared.frontmostApplication, front != source {
                        // Activating the source can take longer than opening the stream.
                        if front == .current, firstFrameMS == nil { continue }
                        stop(message: "The source app changed. Review the area captured so far.")
                        break
                    }
                    let update = await assembler.append(frame)
                    guard recording, !Task.isCancelled else { break }
                    await updateProgress(update)
                }
            } catch {
                if recording { stop(message: "Screen capture was interrupted.") }
            }
        }
        deadline = Task { [weak self] in
            try? await Task.sleep(for: .seconds(5))
            guard let self, !Task.isCancelled, firstFrameMS == nil else { return }
            stop(message: "No frame arrived. Try selecting the area again.")
        }
    }

    func appActivated(_ app: NSRunningApplication) {
        if recording, app != source, app != .current || firstFrameMS != nil {
            stop(message: "The source app changed. Review the area captured so far.")
        }
    }

    func close() {
        recording = false
        automatic?.cancel()
        frames?.cancel()
        deadline?.cancel()
        completing?.cancel()
        if let stream { Task { await stream.stop() } }
        stream = nil
        panel.delegate = nil
        panel.close()
        guide.close()
        report(.cancelled)
    }

    private func updateProgress(_ progress: ScrollAssembly.Progress) async {
        received += 1
        switch progress.update {
        case .initial:
            accepted += 1
            firstFrameMS = RichFilesDiagnostics.milliseconds(since: started)
            deadline?.cancel()
            stopButton.isEnabled = true
            status.stringValue = instructions
            if mode != .manual {
                automatic = Task { [weak self] in await self?.driveScroll() }
            }
        case .moved:
            accepted += 1
            hasUnmatchedEnd = false
            status.stringValue = instructions
        case .unchanged:
            hasUnmatchedEnd = false
            status.stringValue = instructions
        case .unmatched:
            unmatched += 1
            hasUnmatchedEnd = true
            if mode == .manual {
                status.stringValue = "Scroll back until the captured area overlaps, then scroll slowly."
            }
        case .limit:
            stop(message: "The image reached the scrolling capture size limit.")
            return
        }
        sizeLabel.stringValue = "\(Int(progress.size.width)) × \(Int(progress.size.height)) px"
        let now = ProcessInfo.processInfo.systemUptime
        if progress.update != .unchanged, now - lastPreview > 0.35 {
            lastPreview = now
            let preview = await assembler.preview()
            if recording, let preview { thumbnail.image = NSImage(cgImage: preview, size: .zero) }
        }
    }

    private var instructions: String {
        mode == .manual ? "Scroll the selected area, then click Stop." : "Scrolling the selected area…"
    }

    private func driveScroll() async {
        var horizontal = mode != .down
        var direction: Int32 = -1
        var inactive = 0
        let until = ContinuousClock.now + .seconds(120)
        while recording, !Task.isCancelled, ContinuousClock.now < until {
            if source.isTerminated || NSWorkspace.shared.frontmostApplication != source {
                stop(message: "The source app changed. Review the area captured so far.")
                return
            }
            let before = accepted
            let bad = unmatched
            let dimension = horizontal ? guide.frame.width : guide.frame.height
            let delta = direction * Int32(max(4, min(220, dimension * 0.28)))
            guard let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2,
                                      wheel1: horizontal ? 0 : delta, wheel2: horizontal ? delta : 0, wheel3: 0),
                  CGPreflightPostEventAccess() else {
                stop(message: "Automatic scrolling needs Accessibility access.")
                return
            }
            event.location = anchor
            event.post(tap: .cghidEventTap)
            try? await Task.sleep(for: .milliseconds(340))
            guard recording, !Task.isCancelled else { return }
            if accepted == before, unmatched > bad {
                try? await Task.sleep(for: .milliseconds(240))
                guard recording, !Task.isCancelled else { return }
                if accepted == before {
                    stop(message: "The last scroll could not be stitched. Review the area captured so far.")
                    return
                }
            }
            if accepted == before {
                inactive += 1
                guard inactive == 2 else { continue }
                if mode == .both, horizontal {
                    horizontal = false
                    direction = -1
                    inactive = 0
                    continue
                }
                stop(captureLastFrame: true)
                return
            }
            inactive = 0
            if mode == .both, !horizontal {
                horizontal = true
                // Alternate horizontal sweeps at successive vertical positions.
                nextSweep *= -1
                direction = nextSweep
            }
        }
        if recording, !Task.isCancelled { stop(message: "Automatic scrolling reached its time limit.") }
    }

    private var nextSweep: Int32 = -1

    @objc private func stopPressed() { stop(captureLastFrame: true) }
    @objc private func cancelPressed() { cancel?() }
    @objc private func copyPressed() { copy?() }
    @objc private func savePressed() { save?() }

    private func stop(message: String? = nil, captureLastFrame: Bool = false) {
        guard recording else { return }
        recording = false
        automatic?.cancel()
        deadline?.cancel()
        frames?.cancel()
        stopButton.isEnabled = false
        guide.orderOut(nil)
        status.stringValue = "Preparing the image…"
        let closing = stream
        stream = nil
        completing = Task { [weak self] in
            guard let self else { return }
            if let closing { await closing.stop() }
            guard !Task.isCancelled else { return }
            if captureLastFrame, NSWorkspace.shared.frontmostApplication == source,
               let window = CGWindowID(exactly: panel.windowNumber),
               let final = try? await CaptureFrame.read(displayID: displayID, size: displaySize, excluding: [window]),
               NSWorkspace.shared.frontmostApplication == source, !Task.isCancelled,
               let selected = final.cropping(to: crop) {
                // Coalesced stream delivery can leave the last paint pending when Stop is clicked.
                let progress = await assembler.append(selected)
                received += 1
                if case .moved = progress.update { accepted += 1 }
                hasUnmatchedEnd = progress.update == .unmatched || progress.update == .limit
            }
            let image = await assembler.finish()
            guard !Task.isCancelled else { return }
            let detail = message ?? (hasUnmatchedEnd ? "The last view could not be stitched. Review the area captured so far." : nil)
            report(image == nil || detail != nil ? .issues : .complete)
            showResult(image, message: detail)
        }
    }

    func showError(_ message: String) { status.stringValue = message }

    private func showResult(_ image: CGImage?, message: String?) {
        result = image
        status.stringValue = message ?? (accepted < 2 ? "Only the visible area was captured." : "Scrolling capture ready.")
        if let image {
            sizeLabel.stringValue = "\(image.width) × \(image.height) px"
            let panelSize = CGSize(width: min(700, visible.width - 28), height: min(610, visible.height - 28))
            let preview = NSScrollView()
            preview.hasVerticalScroller = true
            preview.autohidesScrollers = true
            let width = min(CGFloat(image.width), panelSize.width - 56)
            let height = CGFloat(image.height) * width / CGFloat(image.width)
            let document = ScrollPreview(frame: CGRect(x: 0, y: 0, width: width, height: height))
            let imageView = NSImageView(frame: document.bounds)
            imageView.image = NSImage(cgImage: image, size: .zero)
            imageView.imageScaling = .scaleAxesIndependently
            document.addSubview(imageView)
            preview.documentView = document
            preview.setAccessibilityLabel("Scrolling screenshot preview")
            preview.widthAnchor.constraint(equalToConstant: panelSize.width - 32).isActive = true
            preview.heightAnchor.constraint(equalToConstant: max(80, panelSize.height - 130)).isActive = true
            status.preferredMaxLayoutWidth = panelSize.width - 32
            let copy = NSButton(title: "Copy Image", target: self, action: #selector(copyPressed))
            copy.keyEquivalent = "\r"
            let save = NSButton(title: "Save PNG…", target: self, action: #selector(savePressed))
            save.keyEquivalent = "s"
            save.keyEquivalentModifierMask = .command
            install(rows: [status, sizeLabel, preview, NSStackView(views: [
                NSButton(title: "Done", target: self, action: #selector(cancelPressed)), save, copy
            ])])
        } else {
            install(rows: [status, NSButton(title: "Close", target: self, action: #selector(cancelPressed))])
        }
        panel.center()
        panel.makeKeyAndOrderFront(nil)
    }

    private func install(rows: [NSView]) {
        // Existing views can move from the live toolbar into the finished preview.
        for row in rows { row.removeFromSuperview() }
        let stack = NSStackView(views: rows)
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 14, bottom: 12, right: 14)
        panel.contentView = stack
        panel.setContentSize(stack.fittingSize)
    }

    private func report(_ outcome: RichFilesDiagnostics.Outcome) {
        guard !reported else { return }
        reported = true
        RichFilesDiagnostics.record(.glanceScrollCapture, id: id, outcome: outcome, started: started,
                                    total: received, confirmed: accepted, firstRowsMS: firstFrameMS)
    }
}

private final class ScrollPanel: NSPanel {
    var cancel: (() -> Void)?
    override var canBecomeKey: Bool { true }
    override func cancelOperation(_ sender: Any?) { cancel?() }
}

private final class ScrollGuide: NSView {
    override func draw(_ dirtyRect: CGRect) {
        let path = NSBezierPath(roundedRect: bounds.insetBy(dx: 1, dy: 1), xRadius: 4, yRadius: 4)
        NSColor(srgbRed: 0.20, green: 0.14, blue: 0.03, alpha: 1).setStroke()
        path.lineWidth = 3
        path.stroke()
        NSColor(srgbRed: 1, green: 0.76, blue: 0.10, alpha: 1).setStroke()
        path.lineWidth = 1
        path.stroke()
    }
}

private final class ScrollPreview: NSView {
    override var isFlipped: Bool { true }
}

private actor ScrollAssembly {
    struct Progress: Sendable {
        let update: ScrollStitcher.Update
        let size: CGSize
    }

    private var stitcher = ScrollStitcher()

    func append(_ image: CGImage) -> Progress {
        let update = stitcher.append(image)
        let bounds = stitcher.bounds
        return Progress(update: update, size: bounds.isNull ? .zero : bounds.size)
    }

    func preview() -> CGImage? { stitcher.image(previewLimit: 136) }

    func finish() -> CGImage? {
        let image = stitcher.image()
        stitcher = ScrollStitcher()
        return image
    }
}
