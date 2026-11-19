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
    private let regionSize: CGSize
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
    private var latestAutomaticFrame: CGImage?
    private var deadline: Task<Void, Never>?
    private var completing: Task<Void, Never>?
    private let started = ProcessInfo.processInfo.systemUptime
    private let id: UUID
    private var firstFrameMS: Int?
    private var received = 0
    private var accepted = 0
    private var movingShare: CGSize?
    private var lastPreview = 0.0
    private var reported = false
    private var recording = true

    private(set) var result: CGImage?
    var reviewWindow: NSWindow? { result == nil ? nil : panel }
    var copy: (() -> Void)?
    var save: (() -> Void)?
    var cancel: (() -> Void)?
    var retry: (() -> Void)?

    init(mode: Mode, source: NSRunningApplication, anchor: CGPoint, displayID: CGDirectDisplayID,
         displaySize: CGSize, crop: CGRect, region: CGRect, visible: CGRect, id: UUID) {
        self.mode = mode
        self.source = source
        self.anchor = anchor
        self.displayID = displayID
        self.displaySize = displaySize
        self.crop = crop
        regionSize = region.size
        self.visible = visible
        self.id = id
        // Keep the rounded stroke and its antialiasing entirely outside the captured pixels.
        guide = NSWindow(contentRect: region.insetBy(dx: -6, dy: -6), styleMask: .borderless,
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
        panel.hasShadow = false
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
        let y = min(max(visible.minY + 12, region.midY - size.height / 2), visible.maxY - size.height - 12)
        let choices = [
            CGPoint(x: x, y: region.minY - size.height - 10),
            CGPoint(x: x, y: region.maxY + 10),
            CGPoint(x: region.minX - size.width - 10, y: y),
            CGPoint(x: region.maxX + 10, y: y)
        ]
        let position = choices.first { visible.insetBy(dx: 8, dy: 8).contains(CGRect(origin: $0, size: size)) }
        panel.setFrameOrigin(position ?? CGPoint(x: x, y: visible.minY + 12))
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
                // The menu and capture overlay have just closed. Let the source finish repainting
                // before locking the baseline. The stream buffers the latest complete frame.
                try await Task.sleep(for: .milliseconds(180))
                for try await frame in opened.images {
                    guard recording, !Task.isCancelled else { break }
                    if let front = NSWorkspace.shared.frontmostApplication, front != source {
                        // Activating the source can take longer than opening the stream.
                        if front == .current, firstFrameMS == nil { continue }
                        stop(.sourceChanged)
                        break
                    }
                    if mode != .manual, firstFrameMS != nil {
                        latestAutomaticFrame = frame
                    } else {
                        _ = await verify(frame)
                    }
                }
            } catch {
                if recording { stop(.streamInterrupted) }
            }
        }
        deadline = Task { [weak self] in
            try? await Task.sleep(for: .seconds(5))
            guard let self, !Task.isCancelled, firstFrameMS == nil else { return }
            stop(.noFrame)
        }
    }

    func appActivated(_ app: NSRunningApplication) {
        if recording, app != source, app != .current || firstFrameMS != nil {
            stop(.sourceChanged)
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

    private func verify(_ frame: CGImage, resampling: Bool = false) async -> ScrollStitcher.Update {
        let progress = await assembler.append(frame, resampling: resampling)
        if recording, !Task.isCancelled { await updateProgress(progress) }
        return progress.update
    }

    private func updateProgress(_ progress: ScrollAssembly.Progress) async {
        received += 1
        movingShare = progress.moving ?? movingShare
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
            status.stringValue = instructions
        case .unchanged:
            status.stringValue = instructions
        case .unmatched, .conflict:
            if mode == .manual {
                status.stringValue = "Return to the last captured view or click Stop to try again."
            }
        case .limit:
            stop(.sizeLimit)
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
                stop(.sourceChanged)
                return
            }
            let before = accepted
            let dimension = horizontal ? regionSize.width : regionSize.height
            let share = movingShare.map { Double(horizontal ? $0.width : $0.height) }
            let delta = direction * Int32(ScrollStitcher.automaticStep(along: Double(dimension), moving: share).rounded())
            guard let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2,
                                      wheel1: horizontal ? 0 : delta, wheel2: horizontal ? delta : 0, wheel3: 0),
                  CGPreflightPostEventAccess() else {
                stop(.accessibility)
                return
            }
            event.location = anchor
            event.post(tap: .cghidEventTap)
            try? await Task.sleep(for: .milliseconds(340))
            guard recording, !Task.isCancelled else { return }
            // Take one more sample before giving up on a wheel step whose first paint has no
            // reliable placement. A pixel conflict at a known placement still refuses export.
            if let frame = latestAutomaticFrame {
                latestAutomaticFrame = nil
                var update = await verify(frame, resampling: true)
                if update == .unmatched {
                    try? await Task.sleep(for: .milliseconds(240))
                    guard recording, !Task.isCancelled else { return }
                    let settled = latestAutomaticFrame ?? frame
                    latestAutomaticFrame = nil
                    update = await verify(settled)
                }
                guard recording, !Task.isCancelled else { return }
                if update == .unmatched || update == .conflict {
                    stop(captureLastFrame: true)
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
        if recording, !Task.isCancelled { stop(.timeLimit) }
    }

    private var nextSweep: Int32 = -1

    @objc private func stopPressed() { stop(captureLastFrame: true) }
    @objc private func cancelPressed() { cancel?() }
    @objc private func retryPressed() { retry?() }
    @objc private func copyPressed() { copy?() }
    @objc private func savePressed() { save?() }

    private typealias Failure = RichFilesDiagnostics.ScrollCaptureFailure

    private func stop(_ failure: Failure? = nil, captureLastFrame: Bool = false) {
        guard recording else { return }
        recording = false
        automatic?.cancel()
        deadline?.cancel()
        frames?.cancel()
        stopButton.isEnabled = false
        guide.orderOut(nil)
        panel.orderOut(nil)
        let closing = stream
        stream = nil
        completing = Task { [weak self] in
            guard let self else { return }
            if let closing { await closing.stop() }
            guard !Task.isCancelled else { return }
            var failure = failure
            if captureLastFrame {
                if NSWorkspace.shared.frontmostApplication != source {
                    failure = .sourceChanged
                } else if let final = try? await ScrollFrameStream.snapshot(
                            displayID: displayID, size: displaySize, crop: crop, excluding: []),
                          NSWorkspace.shared.frontmostApplication == source {
                    // Read the current view again so coalesced or late delivery cannot hide a change.
                    guard !Task.isCancelled else { return }
                    let progress = await assembler.append(final, final: true)
                    received += 1
                    if case .moved = progress.update { accepted += 1 }
                } else {
                    failure = .finalFrameUnavailable
                }
            }
            let (image, verificationFailure) = await assembler.finish(
                mayExport: failure == nil, requireComplete: mode != .manual)
            guard !Task.isCancelled else { return }
            let detail = failure ?? verificationFailure
            report(detail == nil ? .complete : .issues, failure: detail)
            showResult(image, message: detail.map(Self.failureMessage))
        }
    }

    private static func failureMessage(_ failure: Failure) -> String {
        switch failure {
        case .conflictingPixels:
            "The selected area changed while scrolling and could not be stitched. Select just the scrolling content and try again."
        case .lostOverlap:
            "The views could not be stitched. Select just the scrolling content and try scrolling more slowly."
        case .incompleteCoverage:
            "Some of the selected area could not be stitched. Try a smaller selection or capture in one direction."
        case .noMotion:
            "The selected area did not scroll. Try selecting an area that can scroll."
        case .sourceChanged:
            "The source app changed during capture. Try again with the source app active."
        case .streamInterrupted, .noFrame, .finalFrameUnavailable:
            "RichGlance could not verify the screen capture. Try selecting the area again."
        case .sizeLimit, .timeLimit:
            "The capture reached its limit. Try a smaller capture or click Stop sooner."
        case .accessibility:
            "Automatic scrolling needs Accessibility access. Try scrolling manually."
        }
    }

    func showError(_ message: String) { status.stringValue = message }

    private func showResult(_ image: CGImage?, message: String?) {
        result = image
        status.stringValue = message ?? "Scrolling capture ready."
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
            let retry = NSButton(title: "Try Again…", target: self, action: #selector(retryPressed))
            retry.keyEquivalent = "\r"
            install(rows: [status, NSStackView(views: [
                NSButton(title: "Close", target: self, action: #selector(cancelPressed)), retry
            ])])
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

    private func report(_ outcome: RichFilesDiagnostics.Outcome, failure: Failure? = nil) {
        guard !reported else { return }
        reported = true
        RichFilesDiagnostics.record(.glanceScrollCapture, id: id, outcome: outcome, started: started,
                                    total: received, confirmed: accepted, firstRowsMS: firstFrameMS, scrollCaptureFailure: failure)
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
        let moving: CGSize?
    }

    private var stitcher = ScrollStitcher()

    func append(_ image: CGImage, final: Bool = false, resampling: Bool = false) -> Progress {
        let update = stitcher.append(image, final: final, resampling: resampling)
        let bounds = stitcher.bounds
        return Progress(update: update, size: bounds.isNull ? .zero : bounds.size, moving: stitcher.movingShare)
    }

    func preview() -> CGImage? { stitcher.image(previewLimit: 136) }

    func finish(mayExport: Bool, requireComplete: Bool) -> (CGImage?, RichFilesDiagnostics.ScrollCaptureFailure?) {
        defer { stitcher = ScrollStitcher() }
        guard mayExport else { return (nil, nil) }
        switch stitcher.issue {
        case .conflict: return (nil, .conflictingPixels)
        case .unmatched: return (nil, .lostOverlap)
        case .limit: return (nil, .sizeLimit)
        default: break
        }
        guard stitcher.movingShare != nil else { return (nil, .noMotion) }
        guard let image = stitcher.image(requireComplete: requireComplete) else { return (nil, .incompleteCoverage) }
        return (image, nil)
    }
}
