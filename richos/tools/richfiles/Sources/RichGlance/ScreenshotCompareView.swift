import AppKit
import RichFilesCore

@MainActor
final class ScreenshotCompareView: NSView {
    private enum Mode: Int {
        case split, sideBySide, flip, changed
    }

    var leave: (() -> Void)?
    var choose: ((UUID) -> Void)?
    var zoomChanged: ((Int) -> Void)?
    private var mode = Mode.split
    private var fraction: CGFloat = 0.5
    private var showsBefore = false
    private var zoom: CGFloat?
    private var pan = CGPoint.zero
    private var dividerDrag = false
    private var lastPoint: CGPoint?
    private var before: ScreenshotDocument
    private let after: ScreenshotDocument
    private var beforeImage: NSImage?
    private let afterBitmap: CGImage
    private let afterImage: NSImage
    private var diff: ScreenshotComparison?
    private var diffImage: NSImage?
    private var calculation: Task<Void, Never>?
    private var areaIndex: Int?
    private let bar = ScreenshotSurface(.floating, radius: 14, outline: true)
    private let modes = ScreenshotChoice(["Split", "Side by Side", "Before / After", "Changed Pixels"], label: "Compare view")
    private let baselines = NSPopUpButton()
    private let done = ScreenshotButton("Done", hint: "esc", style: .primary)
    private let next = ScreenshotButton("Next Change", hint: "N")
    private let status = screenshotLabel()
    private let beforeLabel = screenshotLabel(weight: .semibold)
    private let afterLabel = screenshotLabel(weight: .semibold)
    private var canvas = CGRect.zero
    private var scale: CGFloat = 1

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    init?(before: ScreenshotDocument, after: ScreenshotDocument, entries: [ScreenshotStore.Entry]) {
        guard let bitmap = ScreenshotRenderer.forComparison(after) else { return nil }
        self.before = before
        self.after = after
        afterBitmap = bitmap
        afterImage = NSImage(cgImage: bitmap, size: after.crop.size)
        super.init(frame: .zero)
        setAccessibilityLabel("Compare screenshots")
        addSubview(bar)
        for control in [modes, baselines, status, next, done] { bar.addSubview(control) }
        for label in [beforeLabel, afterLabel] {
            label.alignment = .center
            label.wantsLayer = true
            label.layer?.cornerRadius = 11
            addSubview(label)
        }
        baselines.font = .systemFont(ofSize: 16)
        baselines.setAccessibilityLabel("Choose Before capture")
        baselines.target = self
        baselines.action = #selector(baselineChanged)
        modes.changed = { [weak self] in if let mode = Mode(rawValue: $0) { self?.setMode(mode) } }
        done.onPress = { [weak self] in self?.leave?() }
        next.onPress = { [weak self] in self?.nextChange() }
        setBefore(before, entries: entries)
    }

    required init?(coder: NSCoder) { nil }

    var beforeID: UUID { before.id }
    var afterID: UUID { after.id }
    private var sameSize: Bool { before.crop.size == after.crop.size }

    func setBefore(_ document: ScreenshotDocument, entries: [ScreenshotStore.Entry]) {
        calculation?.cancel()
        calculation = nil
        before = document
        beforeImage = ScreenshotRenderer.forComparison(document).map { NSImage(cgImage: $0, size: document.crop.size) }
        diff = nil
        diffImage = nil
        areaIndex = nil
        baselines.removeAllItems()
        addBaseline(id: document.id, date: document.created)
        for entry in entries where entry.id != after.id && entry.id != before.id {
            addBaseline(id: entry.id, date: entry.created)
        }
        baselines.selectItem(at: 0)
        refresh()
    }

    private func addBaseline(id: UUID, date: Date) {
        let time = date.formatted(date: .omitted, time: .shortened)
        let title = "Before: \(time)"
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.representedObject = id
        baselines.menu?.addItem(item)
    }

    @objc private func baselineChanged() {
        guard let id = baselines.selectedItem?.representedObject as? UUID else { return }
        choose?(id)
    }

    func stop() { calculation?.cancel() }

    private func setMode(_ mode: Mode) {
        self.mode = mode
        refresh()
        window?.makeFirstResponder(self)
    }

    private func refresh() {
        if mode == .changed, sameSize, diff == nil, calculation == nil,
           let first = ScreenshotRenderer.forComparison(before) {
            let after = afterBitmap
            let started = ProcessInfo.processInfo.systemUptime
            let worker = Task.detached(priority: .userInitiated) { ScreenshotComparison.pixels(before: first, after: after) }
            calculation = Task { [weak self] in
                let result = await withTaskCancellationHandler { await worker.value } onCancel: { worker.cancel() }
                guard !Task.isCancelled, let self else { return }
                calculation = nil
                diff = result
                if let result {
                    diffImage = NSImage(cgImage: result.overlay, size: NSSize(width: after.width, height: after.height))
                }
                RichFilesDiagnostics.record(.glanceCompare, id: UUID(), outcome: result == nil ? .issues : .complete,
                                            started: started, total: 1, confirmed: result == nil ? 0 : 1)
                updateControls()
            }
        }
        updateControls()
    }

    private func updateControls() {
        modes.selection = mode.rawValue
        next.isHidden = mode != .changed || diff?.areas.isEmpty != false
        beforeLabel.stringValue = caption(before, prefix: "Before")
        afterLabel.stringValue = caption(after, prefix: "After")
        let showingBefore = mode == .flip && showsBefore
        beforeLabel.isHidden = mode == .changed || (mode == .flip && !showingBefore)
        afterLabel.isHidden = showingBefore
        if !sameSize {
            let sizes = "\(dimensions(before)) vs \(dimensions(after))"
            let scaleNote = before.scale == after.scale ? "" : " · \(before.scale.formatted())× to \(after.scale.formatted())×"
            status.stringValue = mode == .changed
                ? "Changed Pixels needs equal sizes. \(sizes)\(scaleNote). Choose Side by Side or another Before."
                : "\(sizes)\(scaleNote). Visual comparison."
        } else if mode == .changed {
            if let diff {
                if let areaIndex {
                    let area = diff.areas[areaIndex]
                    let size = area.width == 1 && area.height == 1 ? "1 px" : "\(Int(area.width)) × \(Int(area.height)) px"
                    status.stringValue = "Change \(areaIndex + 1) of \(diff.areas.count) · \(size)"
                } else {
                    status.stringValue = "\(diff.changedPixels.formatted()) px changed in \(diff.areas.count) areas"
                }
            } else {
                status.stringValue = calculation == nil ? "Could not compare these pixels." : "Comparing pixels…"
            }
        } else { status.stringValue = "" }
        status.toolTip = status.stringValue
        setAccessibilityValue(status.stringValue)
        needsLayout = true
        needsDisplay = true
    }

    private func dimensions(_ document: ScreenshotDocument) -> String {
        "\(Int(document.crop.width)) × \(Int(document.crop.height)) px"
    }

    private func caption(_ document: ScreenshotDocument, prefix: String) -> String {
        "\(prefix) · \(document.created.formatted(date: .omitted, time: .shortened)) · \(dimensions(document))"
    }

    override func layout() {
        super.layout()
        let p = ScreenshotEditorPalette(self)
        status.textColor = p.ink
        for label in [beforeLabel, afterLabel] {
            label.textColor = .black
            label.layer?.backgroundColor = NSColor.white.cgColor
        }
        let width = max(1, bounds.width - 24)
        let modesWidth = min(width - 20, modes.naturalWidth)
        let controlsWrap = modesWidth + 265 > width
        let showStatus = !status.stringValue.isEmpty
        let controlsHeight: CGFloat = controlsWrap ? 80 : 43
        let height = controlsHeight + (showStatus ? 36 : 0)
        bar.frame = CGRect(x: 12, y: 8, width: width, height: height)
        modes.frame = CGRect(x: 8, y: 4, width: modesWidth, height: 34)
        let row: CGFloat = controlsWrap ? 43 : 6
        baselines.frame = CGRect(x: controlsWrap ? 8 : modes.frame.maxX + 8, y: row, width: 167, height: 30)
        done.frame = CGRect(x: width - done.naturalWidth() - 8, y: row, width: done.naturalWidth(), height: 31)
        next.frame = CGRect(x: width - next.naturalWidth() - 10, y: controlsHeight + 1, width: next.naturalWidth(), height: 30)
        status.frame = CGRect(x: 12, y: controlsHeight + 7, width: width - (next.isHidden ? 24 : next.naturalWidth() + 35), height: 23)
        canvas = CGRect(x: 0, y: bar.frame.maxY + 36, width: bounds.width, height: max(1, bounds.height - bar.frame.maxY - 36))
        let captionWidth = min(274, max(1, canvas.width / 2 - 16))
        beforeLabel.frame = CGRect(x: 12, y: bar.frame.maxY + 8, width: captionWidth, height: 22)
        afterLabel.frame = CGRect(x: canvas.width - captionWidth - 12, y: beforeLabel.frame.minY, width: captionWidth, height: 22)
        updateScale()
    }

    private func updateScale() {
        let cell = pane(false)
        let availableWidth = mode == .sideBySide ? min(pane(true).width, cell.width) : cell.width
        let width = max(before.crop.width, after.crop.width)
        let height = max(before.crop.height, after.crop.height)
        let backing = window?.backingScaleFactor ?? 1
        scale = zoom.map { $0 / backing } ?? min(1 / backing, max(1, availableWidth - 32) / width, max(1, cell.height - 28) / height)
        zoomChanged?(Int((scale * backing * 100).rounded()))
        needsDisplay = true
    }

    private func pane(_ before: Bool) -> CGRect {
        guard mode == .sideBySide else { return canvas }
        let width = canvas.width / 2
        return CGRect(x: before ? 0 : width, y: canvas.minY, width: width, height: canvas.height)
    }

    private func destination(_ size: CGSize, before: Bool) -> CGRect {
        let cell = pane(before)
        let fit = sameSize ? 1 : min(max(self.before.crop.width, after.crop.width) / size.width,
                                      max(self.before.crop.height, after.crop.height) / size.height)
        let extent = CGSize(width: size.width * scale * fit, height: size.height * scale * fit)
        return CGRect(x: cell.midX - extent.width / 2 + pan.x, y: cell.midY - extent.height / 2 + pan.y,
                      width: extent.width, height: extent.height)
    }

    override func draw(_ dirtyRect: CGRect) {
        let palette = ScreenshotEditorPalette(self)
        palette.ground.setFill()
        dirtyRect.fill()
        guard let context = NSGraphicsContext.current?.cgContext, let beforeImage else { return }
        NSGraphicsContext.current?.imageInterpolation = .none
        let lhs = destination(beforeImage.size, before: true)
        let rhs = destination(afterImage.size, before: false)
        context.saveGState()
        context.clip(to: canvas)
        defer { context.restoreGState() }
        switch mode {
        case .split:
            let x = canvas.width * fraction
            paint(beforeImage, in: lhs, clippedTo: CGRect(x: 0, y: canvas.minY, width: x, height: canvas.height))
            paint(afterImage, in: rhs, clippedTo: CGRect(x: x, y: canvas.minY, width: canvas.width - x, height: canvas.height))
            context.setFillColor(NSColor.black.cgColor)
            context.fill(CGRect(x: x - 2, y: canvas.minY, width: 4, height: canvas.height))
            context.setFillColor(NSColor.white.cgColor)
            context.fill(CGRect(x: x - 1, y: canvas.minY, width: 2, height: canvas.height))
            let handle = CGRect(x: x - 20, y: canvas.midY - 20, width: 40, height: 40)
            context.fillEllipse(in: handle)
            context.setStrokeColor(NSColor.black.cgColor)
            context.strokeEllipse(in: handle)
            ("‹ ›" as NSString).draw(at: CGPoint(x: x - 9, y: handle.minY + 7),
                                     withAttributes: [.font: NSFont.systemFont(ofSize: 18, weight: .semibold), .foregroundColor: NSColor.black])
        case .sideBySide:
            paint(beforeImage, in: lhs, clippedTo: pane(true))
            paint(afterImage, in: rhs, clippedTo: pane(false))
        case .flip:
            paint(showsBefore ? beforeImage : afterImage, in: showsBefore ? lhs : rhs)
        case .changed:
            guard sameSize else { return }
            paint(afterImage, in: rhs, opacity: 0.38)
            if let diffImage { paint(diffImage, in: rhs) }
            if (zoom ?? 0) >= 8 { drawGrid(rhs, context: context) }
            for (index, area) in (diff?.areas ?? []).enumerated() {
                let ring = CGRect(x: rhs.minX + area.minX * scale, y: rhs.minY + area.minY * scale,
                                  width: area.width * scale, height: area.height * scale).insetBy(dx: -2, dy: -2)
                context.setLineWidth(4)
                context.setStrokeColor(NSColor.black.cgColor)
                context.stroke(ring)
                context.setLineWidth(2)
                context.setStrokeColor((index == areaIndex ? NSColor.systemYellow : NSColor.white).cgColor)
                context.stroke(ring)
            }
        }
    }

    private func drawGrid(_ rect: CGRect, context: CGContext) {
        let visible = canvas.intersection(rect)
        guard !visible.isEmpty, scale > 0 else { return }
        context.setStrokeColor(NSColor.black.withAlphaComponent(0.55).cgColor)
        context.setLineWidth(0.5)
        for x in stride(from: rect.minX + ceil((visible.minX - rect.minX) / scale) * scale, through: visible.maxX, by: scale) {
            context.move(to: CGPoint(x: x, y: visible.minY))
            context.addLine(to: CGPoint(x: x, y: visible.maxY))
        }
        for y in stride(from: rect.minY + ceil((visible.minY - rect.minY) / scale) * scale, through: visible.maxY, by: scale) {
            context.move(to: CGPoint(x: visible.minX, y: y))
            context.addLine(to: CGPoint(x: visible.maxX, y: y))
        }
        context.strokePath()
    }

    private func paint(_ image: NSImage, in rect: CGRect, clippedTo clip: CGRect? = nil, opacity: CGFloat = 1) {
        NSGraphicsContext.saveGraphicsState()
        if let clip { NSBezierPath(rect: clip).addClip() }
        image.draw(in: rect, from: .zero, operation: .sourceOver, fraction: opacity, respectFlipped: true, hints: nil)
        NSGraphicsContext.restoreGraphicsState()
    }

    func setActualSize(_ actual: Bool) {
        zoom = actual ? 1 : nil
        pan = .zero
        updateScale()
    }

    func changeZoom(_ direction: Int) {
        let current = scale * (window?.backingScaleFactor ?? 1)
        zoom = max(0.05, min(32, current * (direction > 0 ? 1.5 : 1 / 1.5)))
        updateScale()
    }

    private func nextChange() {
        guard let diff, !diff.areas.isEmpty else { return }
        let index = ((areaIndex ?? -1) + 1) % diff.areas.count
        areaIndex = index
        let area = diff.areas[index]
        if area.width == 1, area.height == 1 { zoom = 32 }
        updateScale()
        pan = CGPoint(x: (afterImage.size.width / 2 - area.midX) * scale,
                      y: (afterImage.size.height / 2 - area.midY) * scale)
        updateControls()
    }

    func handle(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection([.command, .option, .shift, .control])
        guard flags.isEmpty || flags == .shift else { return false }
        switch event.keyCode {
        case 53: leave?(); return true
        case 49: showsBefore.toggle(); setMode(.flip); return true
        case 123: pan.x += 20
        case 124: pan.x -= 20
        case 125: pan.y -= 20
        case 126: pan.y += 20
        default:
            switch event.charactersIgnoringModifiers?.lowercased() {
            case "1": setMode(.split)
            case "2": setMode(.sideBySide)
            case "3": setMode(.flip)
            case "4": setMode(.changed)
            case "[": fraction = max(0.02, fraction - 0.05)
            case "]": fraction = min(0.98, fraction + 0.05)
            case "n" where mode == .changed: nextChange()
            case "+", "=": changeZoom(1)
            case "-": changeZoom(-1)
            case "0": setActualSize(false)
            default: return false
            }
        }
        needsDisplay = true
        return true
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let point = convert(event.locationInWindow, from: nil)
        dividerDrag = mode == .split && abs(point.x - canvas.width * fraction) < 24
        lastPoint = point
    }

    override func mouseDragged(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        if dividerDrag { fraction = max(0.02, min(0.98, point.x / max(1, canvas.width))) }
        else if let lastPoint {
            pan.x += point.x - lastPoint.x
            pan.y += point.y - lastPoint.y
        }
        lastPoint = point
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) { lastPoint = nil }

    override func scrollWheel(with event: NSEvent) {
        if event.modifierFlags.contains(.control) { changeZoom(event.scrollingDeltaY > 0 ? 1 : -1) }
        else {
            pan.x += event.scrollingDeltaX
            pan.y += event.scrollingDeltaY
            needsDisplay = true
        }
    }
}
