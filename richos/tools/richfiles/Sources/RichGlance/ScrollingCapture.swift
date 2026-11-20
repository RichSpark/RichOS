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
    private let region: CGRect
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
    private var excludedWindows: Set<CGWindowID> = []
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
    private var liveKeys: LiveCaptureKeys?

    private(set) var result: CGImage?
    var reviewWindow: NSWindow? { result == nil ? nil : panel }
    var opensEditor = false
    var copy: (() -> Void)?
    var edit: (() -> Void)?
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
        self.region = region
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
        panel.cancel = { [weak self] in self?.cancelPressed() }
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
        liveKeys = LiveCaptureKeys(escape: { [weak self] in self?.cancelPressed() },
                                   stop: { [weak self] in
            guard let self, stopButton.isEnabled else { return }
            stopPressed()
        })
        guide.orderFrontRegardless()
        panel.orderFrontRegardless()
        excludedWindows = Set([guide, panel].compactMap { CGWindowID(exactly: $0.windowNumber) })
        frames = Task { [weak self] in
            guard let self else { return }
            do {
                let opened = try await ScrollFrameStream.open(displayID: displayID, size: displaySize,
                                                              crop: crop, excluding: excludedWindows)
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
        liveKeys = nil
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
        case .unmatched:
            if mode == .manual {
                status.stringValue = "Scroll back until the views overlap, then scroll slowly."
            }
        case .conflict:
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
        switch mode {
        case .manual: "Scroll the selected area, then click Stop."
        case .down: "Scrolling down…"
        case .right: "Scrolling right…"
        case .both: "Scrolling down and right…"
        }
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
            let dimension = horizontal ? region.width : region.height
            let share = movingShare.map { Double(horizontal ? $0.width : $0.height) }
            let delta = direction * Int32(ScrollStitcher.automaticStep(along: Double(dimension), moving: share).rounded())
            guard CGPreflightPostEventAccess(),
                  let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2,
                                      wheel1: horizontal ? 0 : delta, wheel2: horizontal ? delta : 0, wheel3: 0) else {
                stop(.accessibility)
                return
            }
            latestAutomaticFrame = nil
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
                    stop(update == .conflict ? .conflictingPixels : .lostOverlap)
                    return
                }
            }
            if accepted == before {
                inactive += 1
                guard inactive == 2 else { continue }
                // Silence from the live stream does not establish a scroll boundary. Its last
                // paint may still be queued. Verify a fresh view before switching axes or ending.
                guard let update = await confirmBoundary() else {
                    if recording { stop(.finalFrameUnavailable) }
                    return
                }
                guard recording, !Task.isCancelled else { return }
                if update == .unchanged {
                    if mode == .both, horizontal {
                        horizontal = false
                        direction = -1
                        inactive = 0
                        continue
                    }
                    // This sample already verified the end. A panel overlapping the crop needs
                    // one more check with the controls hidden to guard against compositor leakage.
                    stop(captureLastFrame: panel.frame.intersects(region))
                    return
                }
                if accepted == before {
                    stop(update == .conflict ? .conflictingPixels : .lostOverlap)
                    return
                }
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

    private func confirmBoundary() async -> ScrollStitcher.Update? {
        for attempt in 0...1 {
            guard let image = try? await ScrollFrameStream.snapshot(
                displayID: displayID, size: displaySize, crop: crop, excluding: excludedWindows),
                recording, !Task.isCancelled else { return nil }
            let update = await verify(image, resampling: attempt == 0)
            if attempt == 1 || (update != .unmatched && update != .conflict) { return update }
        }
        return nil
    }

    private var nextSweep: Int32 = -1

    @objc private func stopPressed() { stop(captureLastFrame: true) }
    @objc private func cancelPressed() {
        guard recording || completing != nil else { cancel?(); return }
        recording = false
        liveKeys = nil
        automatic?.cancel()
        frames?.cancel()
        deadline?.cancel()
        completing?.cancel()
        completing = nil
        if let stream { Task { await stream.stop() } }
        stream = nil
        report(.cancelled)
        showResult(nil, message: "Scrolling capture canceled.", canceled: true)
    }
    @objc private func retryPressed() { retry?() }
    @objc private func copyPressed() { copy?() }
    @objc private func editPressed() { edit?() }
    @objc private func savePressed() { save?() }

    private typealias Failure = RichFilesDiagnostics.ScrollCaptureFailure

    private func stop(_ failure: Failure? = nil, captureLastFrame: Bool = false) {
        guard recording else { return }
        recording = false
        liveKeys = nil
        automatic?.cancel()
        deadline?.cancel()
        frames?.cancel()
        stopButton.isEnabled = false
        let controlsCoverSelection = panel.frame.intersects(region)
        if controlsCoverSelection {
            guide.orderOut(nil)
            panel.orderOut(nil)
        } else {
            status.stringValue = "Verifying the capture…"
        }
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
                            displayID: displayID, size: displaySize, crop: crop,
                            excluding: controlsCoverSelection ? [] : excludedWindows),
                          NSWorkspace.shared.frontmostApplication == source {
                    // Read the current view again so coalesced or late delivery cannot hide a change.
                    guard !Task.isCancelled else { return }
                    let progress = await assembler.append(final, final: true)
                    received += 1
                    if case .moved = progress.update { accepted += 1 }
                    if progress.update == .limit { failure = .sizeLimit }
                } else {
                    failure = .finalFrameUnavailable
                }
            }
            let (image, detail) = await assembler.finish(stoppingFor: failure, requireComplete: mode != .manual)
            guard !Task.isCancelled else { return }
            report(detail == nil ? .complete : .issues, failure: detail)
            completing = nil
            showResult(image, message: detail.map { Self.stopMessage($0, hasImage: image != nil) })
        }
    }

    private static func stopMessage(_ failure: Failure, hasImage: Bool) -> String {
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
            hasImage ? "The source app changed. Review the area captured so far."
                : "The source app changed during capture. Try again with the source app active."
        case .streamInterrupted, .noFrame, .finalFrameUnavailable:
            "RichGlance could not verify the screen capture. Try selecting the area again."
        case .sizeLimit:
            hasImage ? "The capture reached the image size limit. Review the area captured so far."
                : "The capture reached the image size limit. Try selecting a smaller area."
        case .timeLimit:
            hasImage ? "Automatic scrolling reached its time limit. Review the area captured so far."
                : "Automatic scrolling reached its time limit. Try selecting a smaller area."
        case .accessibility:
            "Automatic scrolling needs Accessibility access. Try scrolling manually."
        }
    }

    func showError(_ message: String) { status.stringValue = message }

    private func showResult(_ image: CGImage?, message: String?, canceled: Bool = false) {
        guide.orderOut(nil)
        result = image
        status.stringValue = message ?? "Scrolling capture ready."
        var focus: NSView?
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
            let edit = NSButton(title: "Open in Editor", target: self, action: #selector(editPressed))
            if opensEditor { edit.keyEquivalent = "\r" }
            else { copy.keyEquivalent = "\r" }
            panel.performAction = { [weak self] key in
                if key == "c" { self?.copyPressed(); return true }
                if key == "e" { self?.editPressed(); return true }
                return false
            }
            focus = opensEditor ? edit : copy
            let save = NSButton(title: "Save PNG…", target: self, action: #selector(savePressed))
            save.keyEquivalent = "s"
            save.keyEquivalentModifierMask = .command
            install(rows: [status, sizeLabel, preview, NSStackView(views: [
                NSButton(title: "Done", target: self, action: #selector(cancelPressed)), save, edit, copy
            ])])
        } else if canceled {
            let done = NSButton(title: "Done", target: self, action: #selector(cancelPressed))
            done.keyEquivalent = "\r"
            focus = done
            install(rows: [status, done])
        } else {
            let retry = NSButton(title: "Try Again…", target: self, action: #selector(retryPressed))
            retry.keyEquivalent = "\r"
            focus = retry
            install(rows: [status, NSStackView(views: [
                NSButton(title: "Close", target: self, action: #selector(cancelPressed)), retry
            ])])
        }
        let size = panel.frame.size
        let x = min(max(visible.minX + 12, region.midX - size.width / 2), visible.maxX - size.width - 12)
        let y = min(max(visible.minY + 12, region.midY - size.height / 2), visible.maxY - size.height - 12)
        let choices = [
            CGPoint(x: x, y: region.minY - size.height - 10),
            CGPoint(x: x, y: region.maxY + 10),
            CGPoint(x: region.minX - size.width - 10, y: y),
            CGPoint(x: region.maxX + 10, y: y),
        ]
        let origin = choices.first { visible.contains(CGRect(origin: $0, size: size)) } ??
            CGPoint(x: visible.midX - size.width / 2, y: visible.midY - size.height / 2)
        panel.setFrameOrigin(origin)
        panel.makeKeyAndOrderFront(nil)
        if let focus { panel.makeFirstResponder(focus) }
    }

    private func install(rows: [NSView]) {
        // Existing views can move from the live toolbar into the finished preview.
        for row in rows { row.removeFromSuperview() }
        status.font = .systemFont(ofSize: 16)
        sizeLabel.font = .monospacedDigitSystemFont(ofSize: 16, weight: .medium)
        for row in rows {
            let children = (row as? NSStackView)?.arrangedSubviews ?? [row]
            for button in children.compactMap({ $0 as? NSButton }) {
                button.font = .systemFont(ofSize: 16, weight: .medium)
                button.bezelStyle = .rounded
                button.controlSize = .large
            }
        }
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
    var performAction: ((String) -> Bool)?
    override func keyDown(with event: NSEvent) {
        let flags = event.modifierFlags.intersection([.shift, .command, .control, .option])
        if flags.isEmpty, event.keyCode == 53 {
            if !event.isARepeat { cancel?() }
            return
        }
        if flags.isEmpty, let key = event.charactersIgnoringModifiers?.lowercased(), performAction?(key) == true { return }
        super.keyDown(with: event)
    }
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

actor ScrollAssembly {
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

    func finish(stoppingFor stop: RichFilesDiagnostics.ScrollCaptureFailure?, requireComplete: Bool)
        -> (CGImage?, RichFilesDiagnostics.ScrollCaptureFailure?) {
        defer { stitcher = ScrollStitcher() }
        switch stop {
        case nil, .sizeLimit, .timeLimit, .sourceChanged: break
        default: return (nil, stop)
        }
        switch stitcher.issue {
        case .conflict: return (nil, .conflictingPixels)
        case .unmatched: return (nil, .lostOverlap)
        default: break
        }
        guard stitcher.movingShare != nil else { return (nil, stop ?? .noMotion) }
        guard let image = stitcher.image(requireComplete: requireComplete) else {
            return (nil, stop ?? .incompleteCoverage)
        }
        return (image, stop)
    }
}
