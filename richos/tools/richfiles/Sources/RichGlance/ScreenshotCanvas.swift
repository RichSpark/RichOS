import AppKit

@MainActor
final class ScreenshotCanvas: NSView, NSTextViewDelegate {
    enum Zoom { case fit, actual, custom(CGFloat) }
    private enum Handle: Equatable {
        case move, start, end, pointer, corner(Int), edge(Int)
    }
    private struct Gesture {
        let anchor: CGPoint
        let before: ScreenshotEdits
        var mark: ScreenshotMark?
        var crop: CGRect?
        let handle: Handle
    }

    var document: ScreenshotDocument
    var selected: UUID?
    var tool = ScreenshotTool.select {
        didSet {
            if tool == .crop, oldValue != .crop { pendingCrop = document.crop }
            if tool != .crop { pendingCrop = nil }
            refresh()
        }
    }
    var ink: ScreenshotInk { tool == .highlight ? highlightInk : commonInk }
    var large: Bool { sizes[tool] ?? false }
    var commonInk = ScreenshotInk.red
    var highlightInk = ScreenshotInk.yellow
    var sizes: [ScreenshotTool: Bool] = [.arrow: true]
    var isInteracting: Bool { gesture != nil || textEntry != nil }
    var previewCrop: CGRect? { pendingCrop }
    var isTyping: Bool { textEntry != nil }
    var changed: (() -> Void)?
    var keys: ((NSEvent) -> Bool)?
    private let image: NSImage
    private var gesture: Gesture?
    private var tracking: NSTrackingArea?
    private var hover: CGPoint?
    private var hoverHandle: Handle?
    private var textEntry: ScreenshotCalloutField?
    private var beforeTyping: ScreenshotEdits?
    private var originalCallout: ScreenshotMark?
    private var pendingCrop: CGRect?
    struct Revisions {
        var undo: [ScreenshotEdits] = []
        var redo: [ScreenshotEdits] = []
    }
    var revisions = Revisions()
    private let margin: CGFloat = 39
    private(set) var factor: CGFloat = 1
    private var imageOrigin = CGPoint.zero

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override var mouseDownCanMoveWindow: Bool { false }

    init(document: ScreenshotDocument) {
        self.document = document
        image = NSImage(cgImage: document.original, size: document.bounds.size)
        super.init(frame: .zero)
        setAccessibilityElement(true)
        setAccessibilityRole(.image)
        setAccessibilityLabel("Screenshot canvas")
    }

    required init?(coder: NSCoder) { nil }

    var selectedMark: ScreenshotMark? { document.edits.marks.first { $0.id == selected } }
    var canUndo: Bool { !revisions.undo.isEmpty }
    var canRedo: Bool { !revisions.redo.isEmpty }
    var hasChanges: Bool { !document.edits.marks.isEmpty || document.crop != document.bounds }
    var selectionIsOffscreen: Bool {
        guard let selectedMark else { return false }
        return !visibleRect.intersects(viewRect(markBounds(selectedMark)))
    }

    private var shown: CGRect { tool == .crop ? document.bounds : document.crop }
    private var actual: CGFloat { 1 / (window?.backingScaleFactor ?? document.scale) }

    func layoutImage(in viewport: CGSize, zoom: Zoom) {
        let rect = shown
        guard !rect.isEmpty else { return }
        let width = max(1, viewport.width - 2 * margin) / rect.width
        let height = max(1, viewport.height - 2 * margin) / rect.height
        switch zoom {
        case .fit: factor = min(actual, width, height)
        case .actual: factor = actual
        case .custom(let multiplier): factor = actual * multiplier
        }
        let size = CGSize(width: rect.width * factor, height: rect.height * factor)
        let extent = CGSize(width: max(viewport.width, size.width + 2 * margin),
                            height: max(viewport.height, size.height + 2 * margin))
        if frame.size != extent { setFrameSize(extent) }
        imageOrigin = CGPoint(x: ((extent.width - size.width) / 2).rounded() - rect.minX * factor,
                              y: ((extent.height - size.height) / 2).rounded() - rect.minY * factor)
        placeTextEntry()
        needsDisplay = true
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: .zero, options: [.inVisibleRect, .activeInKeyWindow, .mouseMoved, .mouseEnteredAndExited],
                                  owner: self, userInfo: nil)
        tracking = area
        addTrackingArea(area)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        refresh()
    }

    override func draw(_ dirtyRect: CGRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        ScreenshotEditorPalette(self).ground.setFill()
        dirtyRect.fill()
        let imageRect = viewRect(shown)
        context.saveGState()
        context.setShadow(offset: CGSize(width: 0, height: -10), blur: 22,
                          color: NSColor.black.withAlphaComponent(0.29).cgColor)
        context.setFillColor(ScreenshotEditorPalette(self).ground.cgColor)
        context.fill(imageRect)
        context.restoreGState()
        context.saveGState()
        context.clip(to: imageRect)
        NSGraphicsContext.current?.imageInterpolation = factor >= actual ? .none : .high
        image.draw(in: viewRect(document.bounds), from: .zero, operation: .sourceOver,
                   fraction: 1, respectFlipped: true, hints: nil)
        context.translateBy(x: imageOrigin.x, y: imageOrigin.y)
        context.scaleBy(x: factor, y: factor)
        var drawing = document.edits
        if textEntry != nil, let index = drawing.marks.firstIndex(where: { $0.id == selected }) {
            drawing.marks[index].text = ""
        }
        ScreenshotRenderer.drawMarks(drawing, scale: document.scale, in: context)
        if tool == .steps, let hover, shown.contains(hover), gesture == nil {
            context.setAlpha(0.65)
            ScreenshotRenderer.draw(ScreenshotMark(tool: .steps, start: hover, end: hover, ink: ink, large: large),
                                    number: document.edits.marks.filter { $0.tool == .steps }.count + 1,
                                    scale: document.scale, in: context)
        }
        context.restoreGState()
        outline(imageRect, dashed: false, in: context)
        if let crop = pendingCrop {
            let cut = viewRect(crop)
            let shade = CGMutablePath()
            shade.addRect(imageRect)
            shade.addRect(cut)
            context.addPath(shade)
            context.setFillColor(NSColor.black.withAlphaComponent(0.56).cgColor)
            context.drawPath(using: .eoFill)
            outline(cut, dashed: true, in: context)
        }
        drawExportLabel(pendingCrop ?? document.crop)
        if pendingCrop != nil {
            for (handle, point) in handles() { drawHandle(handle, at: viewPoint(point), in: context) }
        }
        if let mark = selectedMark, tool != .crop {
            outline(viewRect(markBounds(mark)), dashed: true, in: context)
            for (handle, point) in handles() { drawHandle(handle, at: viewPoint(point), in: context) }
            if mark.tool == .redact {
                let rect = viewRect(mark.rect)
                let label = NSAttributedString(string: "\(Int(mark.rect.integral.width)) × \(Int(mark.rect.integral.height)) px",
                                               attributes: [.font: NSFont.monospacedDigitSystemFont(ofSize: 16, weight: .semibold),
                                                            .foregroundColor: NSColor.white])
                let size = label.size()
                let above = rect.minY - size.height - 10
                let point = CGPoint(x: max(6, min(rect.minX, bounds.maxX - size.width - 12)),
                                    y: above >= visibleRect.minY ? above : rect.maxY + 10)
                let path = NSBezierPath(roundedRect: CGRect(origin: point, size: size).insetBy(dx: -9, dy: -4),
                                        xRadius: 13, yRadius: 13)
                ScreenshotRenderer.ink.setFill()
                path.fill()
                NSColor.white.setStroke()
                path.stroke()
                label.draw(at: point)
            }
        }
    }

    override func mouseMoved(with event: NSEvent) {
        hover = source(event)
        let handle = handle(at: convert(event.locationInWindow, from: nil))
        if hoverHandle != handle || tool == .steps { needsDisplay = true }
        hoverHandle = handle
    }

    override func mouseExited(with event: NSEvent) {
        hover = nil
        hoverHandle = nil
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        finishCallout()
        window?.makeFirstResponder(self)
        let point = source(event)
        let location = viewPoint(point)
        if event.clickCount == 2, tool == .select, selectedMark?.tool == .text,
           let selectedMark, contains(point, in: selectedMark) {
            startCallout()
            return
        }
        if let nearest = handle(at: location) {
            gesture = Gesture(anchor: point, before: document.edits, mark: selectedMark, crop: pendingCrop, handle: nearest)
        } else if tool == .crop {
            let crop = pendingCrop?.contains(point) == true ? pendingCrop : CGRect(origin: point, size: .zero)
            gesture = Gesture(anchor: point, before: document.edits, crop: crop,
                              handle: crop?.isEmpty == false ? .move : .end)
        } else if tool == .select {
            let top = document.edits.marks.reversed().filter { $0.tool == .redact } +
                document.edits.marks.reversed().filter { $0.tool != .redact }
            selected = top.first { contains(point, in: $0) }?.id
            if let selectedMark { gesture = Gesture(anchor: point, before: document.edits, mark: selectedMark, handle: .move) }
        } else if shown.contains(point) {
            var end = point
            var pointer: CGPoint?
            if tool == .text {
                end = CGPoint(x: min(shown.maxX, point.x + 110 * document.scale),
                              y: min(shown.maxY, point.y + 35 * document.scale))
                pointer = CGPoint(x: max(shown.minX, point.x - 12 * document.scale),
                                  y: min(shown.maxY, end.y + 22 * document.scale))
            }
            let mark = ScreenshotMark(tool: tool, start: point, end: end, pointer: pointer, ink: ink, large: large)
            gesture = Gesture(anchor: point, before: document.edits, handle: .end)
            document.edits.marks.append(mark)
            selected = mark.id
        } else {
            selected = nil
        }
        refresh()
    }

    override func mouseDragged(with event: NSEvent) {
        guard let gesture else { return }
        let point = bounded(source(event), within: shown)
        let dx = point.x - gesture.anchor.x
        let dy = point.y - gesture.anchor.y
        if let crop = gesture.crop {
            pendingCrop = resize(crop, handle: gesture.handle, to: point, dx: dx, dy: dy, within: document.bounds)
        } else if let index = document.edits.marks.firstIndex(where: { $0.id == selected }) {
            var mark = gesture.mark ?? document.edits.marks[index]
            switch gesture.handle {
            case .move:
                let available = shown
                let box = markBounds(mark)
                let x = box.width <= available.width ? min(available.maxX - box.maxX, max(available.minX - box.minX, dx)) : dx
                let y = box.height <= available.height ? min(available.maxY - box.maxY, max(available.minY - box.minY, dy)) : dy
                mark.move(dx: x, dy: y)
            case .start: mark.start = point
            case .end:
                if mark.tool != .steps, mark.tool != .text || gesture.mark != nil { mark.end = point }
            case .pointer: mark.pointer = point
            case .corner, .edge:
                let rect = resize(mark.rect, handle: gesture.handle, to: point, dx: dx, dy: dy, within: shown)
                mark.start = rect.origin
                mark.end = CGPoint(x: rect.maxX, y: rect.maxY)
            }
            document.edits.marks[index] = mark
        }
        refresh()
    }

    override func mouseUp(with event: NSEvent) {
        guard let gesture else { return }
        self.gesture = nil
        if let mark = selectedMark, gesture.mark == nil, gesture.crop == nil {
            if mark.tool != .steps, mark.rect.isEmpty, mark.tool != .arrow || mark.start == mark.end {
                document.edits = gesture.before
                selected = nil
            }
        }
        if gesture.mark == nil, tool == .text, selectedMark?.tool == .text {
            startCallout(before: gesture.before)
        } else {
            commit(gesture.before)
        }
        refresh()
    }

    override func keyDown(with event: NSEvent) {
        if keys?(event) != true { super.keyDown(with: event) }
    }

    func commitCrop() {
        guard let crop = pendingCrop?.integral.intersection(document.bounds), !crop.isEmpty else { return }
        modify { $0.crop = crop }
        tool = .select
    }

    func cancelCrop() { tool = .select }

    func selectNext(backwards: Bool) {
        let marks = document.edits.marks
        guard !marks.isEmpty else { return }
        let index = marks.firstIndex { $0.id == selected } ?? (backwards ? 0 : -1)
        selected = marks[(index + (backwards ? -1 : 1) + marks.count) % marks.count].id
        refresh()
    }

    func setStyle(ink: ScreenshotInk? = nil, large: Bool? = nil) {
        let editedTool = selectedMark?.tool ?? tool
        if let ink {
            if editedTool == .highlight { highlightInk = ink }
            else { commonInk = ink }
        }
        if let large { sizes[editedTool] = large }
        editSelected { mark in
            if let ink { mark.ink = ink }
            if let large { mark.large = large }
        }
        if selectedMark == nil { refresh() }
    }

    func startCallout(before: ScreenshotEdits? = nil) {
        guard textEntry == nil, let mark = selectedMark, mark.tool == .text else { return }
        beforeTyping = before ?? document.edits
        originalCallout = mark
        let text = ScreenshotCalloutField()
        text.isRichText = false
        text.allowsUndo = true
        text.isAutomaticQuoteSubstitutionEnabled = false
        text.isAutomaticDashSubstitutionEnabled = false
        text.drawsBackground = false
        text.isVerticallyResizable = false
        text.isHorizontallyResizable = false
        text.textContainerInset = .zero
        text.textContainer?.lineFragmentPadding = 0
        text.textContainer?.widthTracksTextView = true
        text.setAccessibilityLabel("Callout text")
        text.string = mark.text
        text.delegate = self
        text.finish = { [weak self] in self?.finishCallout() }
        textEntry = text
        addSubview(text)
        placeTextEntry()
        window?.makeFirstResponder(text)
        text.selectAll(nil)
        needsDisplay = true
    }

    func textDidChange(_ notification: Notification) {
        guard let view = textEntry, let selected, let original = originalCallout,
              let index = document.edits.marks.firstIndex(where: { $0.id == selected }) else { return }
        var mark = document.edits.marks[index]
        mark.text = view.string
        let unit = document.scale * (mark.large ? 1.5 : 1)
        let font = NSFont.systemFont(ofSize: 16 * unit, weight: .semibold)
        let widthLimit = max(1, min(360 * unit, shown.maxX - mark.rect.minX))
        let padding = 16 * document.scale
        let maxTextWidth = max(1, widthLimit - padding)
        let measured = (mark.text as NSString).boundingRect(
            with: CGSize(width: maxTextWidth, height: .greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading], attributes: [.font: font]
        )
        let width = min(widthLimit, max(original.rect.width, 65 * unit, measured.width + padding))
        let height = min(shown.maxY - mark.rect.minY,
                         max(original.rect.height, 29 * unit, measured.height + 12 * document.scale))
        mark.start = mark.rect.origin
        mark.end = CGPoint(x: mark.start.x + ceil(width), y: mark.start.y + ceil(height))
        document.edits.marks[index] = mark
        placeTextEntry()
        needsDisplay = true
    }

    func finishCallout() {
        guard let field = textEntry, let before = beforeTyping else { return }
        textEntry = nil
        beforeTyping = nil
        originalCallout = nil
        field.removeFromSuperview()
        if let selected, let index = document.edits.marks.firstIndex(where: { $0.id == selected }) {
            let text = field.string.trimmingCharacters(in: .whitespacesAndNewlines)
            if text.isEmpty {
                document.edits.marks.remove(at: index)
                self.selected = nil
            } else {
                document.edits.marks[index].text = text
            }
        }
        commit(before)
        window?.makeFirstResponder(self)
        refresh()
    }

    private func placeTextEntry() {
        guard let field = textEntry, let mark = selectedMark else { return }
        let font = NSFont.systemFont(ofSize: 16 * document.scale * (mark.large ? 1.5 : 1) * factor, weight: .semibold)
        if field.font != font { field.font = font }
        if field.textColor != mark.ink.outline { field.textColor = mark.ink.outline }
        field.insertionPointColor = mark.ink.outline
        field.frame = viewRect(mark.rect.insetBy(dx: 8 * document.scale, dy: 5 * document.scale))
    }

    func editSelected(_ body: (inout ScreenshotMark) -> Void) {
        guard let index = document.edits.marks.firstIndex(where: { $0.id == selected }) else { return }
        modify { body(&$0.marks[index]) }
    }

    func nudge(dx: CGFloat, dy: CGFloat) {
        if let crop = pendingCrop {
            pendingCrop = resize(crop, handle: .move, to: .zero, dx: dx, dy: dy, within: document.bounds)
            refresh()
            return
        }
        editSelected { $0.move(dx: dx, dy: dy) }
    }

    func deleteSelected() {
        guard let selected else { return }
        self.selected = nil
        modify { $0.marks.removeAll { $0.id == selected } }
    }

    func reorderSelected(by amount: Int) {
        guard let selected else { return }
        modify { $0.reorderStep(selected, by: amount) }
    }

    func revealSelection() {
        if let selectedMark { scrollToVisible(viewRect(markBounds(selectedMark)).insetBy(dx: -18, dy: -18)) }
    }

    func undo() {
        guard let edits = revisions.undo.popLast() else { return }
        revisions.redo.append(document.edits)
        document.edits = edits
        refresh()
    }

    func redo() {
        guard let edits = revisions.redo.popLast() else { return }
        revisions.undo.append(document.edits)
        document.edits = edits
        refresh()
    }

    func refresh() {
        setAccessibilityValue("\(Int(document.crop.width)) × \(Int(document.crop.height)) px")
        needsDisplay = true
        changed?()
    }

    private func commit(_ before: ScreenshotEdits) {
        guard before != document.edits else { return }
        revisions.undo.append(before)
        if revisions.undo.count > 50 { revisions.undo.removeFirst() }
        revisions.redo.removeAll()
    }

    private func modify(_ body: (inout ScreenshotEdits) -> Void) {
        let before = document.edits
        body(&document.edits)
        commit(before)
        refresh()
    }

    private func markBounds(_ mark: ScreenshotMark) -> CGRect {
        if mark.tool == .steps {
            let radius = 15 * document.scale * (mark.large ? 1.5 : 1)
            return CGRect(x: mark.start.x - radius, y: mark.start.y - radius, width: 2 * radius, height: 2 * radius)
        }
        if mark.tool == .arrow { return mark.rect.insetBy(dx: -4 * document.scale, dy: -4 * document.scale) }
        return mark.rect
    }

    private func contains(_ point: CGPoint, in mark: ScreenshotMark) -> Bool {
        if mark.tool == .arrow {
            let dx = mark.end.x - mark.start.x
            let dy = mark.end.y - mark.start.y
            let length = dx * dx + dy * dy
            guard length > 0 else { return false }
            let progress = min(1, max(0, ((point.x - mark.start.x) * dx + (point.y - mark.start.y) * dy) / length))
            return hypot(point.x - mark.start.x - progress * dx, point.y - mark.start.y - progress * dy) <= max(7 / factor, 4)
        }
        return markBounds(mark).contains(point)
    }

    private func handle(at location: CGPoint) -> Handle? {
        handles().first {
            let point = viewPoint($0.1)
            return hypot(point.x - location.x, point.y - location.y) <= 10
        }?.0
    }

    private func handles() -> [(Handle, CGPoint)] {
        if let crop = pendingCrop { return rectHandles(crop, edges: true) }
        guard let mark = selectedMark else { return [] }
        switch mark.tool {
        case .arrow: return [(.start, mark.start), (.end, mark.end)]
        case .text:
            return rectHandles(mark.rect, edges: true) + (mark.pointer.map { [(.pointer, $0)] } ?? [])
        case .box, .redact, .highlight: return rectHandles(mark.rect, edges: false)
        default: return []
        }
    }

    private func rectHandles(_ rect: CGRect, edges: Bool) -> [(Handle, CGPoint)] {
        let corners = [(rect.minX, rect.minY), (rect.maxX, rect.minY), (rect.maxX, rect.maxY), (rect.minX, rect.maxY)]
        let sides = [(rect.midX, rect.minY), (rect.maxX, rect.midY), (rect.midX, rect.maxY), (rect.minX, rect.midY)]
        return corners.enumerated().map { (.corner($0.offset), CGPoint(x: $0.element.0, y: $0.element.1)) } +
            (edges ? sides.enumerated().map { (.edge($0.offset), CGPoint(x: $0.element.0, y: $0.element.1)) } : [])
    }

    private func resize(_ rect: CGRect, handle: Handle, to point: CGPoint, dx: CGFloat, dy: CGFloat,
                        within limits: CGRect) -> CGRect {
        var a = rect.origin
        var b = CGPoint(x: rect.maxX, y: rect.maxY)
        switch handle {
        case .move:
            return rect.offsetBy(dx: min(limits.maxX - rect.maxX, max(limits.minX - rect.minX, dx)),
                                 dy: min(limits.maxY - rect.maxY, max(limits.minY - rect.minY, dy)))
        case .end: b = point
        case .corner(let i):
            if i == 0 || i == 3 { a.x = point.x } else { b.x = point.x }
            if i == 0 || i == 1 { a.y = point.y } else { b.y = point.y }
        case .edge(let i):
            switch i {
            case 0: a.y = point.y
            case 1: b.x = point.x
            case 2: b.y = point.y
            default: a.x = point.x
            }
        default: break
        }
        return ScreenshotRenderer.rect(from: a, to: b).intersection(limits)
    }

    private func drawExportLabel(_ crop: CGRect) {
        let box = viewRect(crop)
        let colors = ScreenshotEditorPalette(self)
        let name = pendingCrop == nil ? "Exported image" : "After the crop"
        let title = NSAttributedString(string: "\(name) · \(Int(crop.integral.width)) × \(Int(crop.integral.height)) px",
                                       attributes: [.font: NSFont.monospacedDigitSystemFont(ofSize: 16, weight: .semibold),
                                                    .foregroundColor: colors.ink])
        let label = CGRect(x: box.minX, y: box.minY - 33, width: title.size().width + 43, height: 28)
        let path = NSBezierPath(roundedRect: label, xRadius: 7, yRadius: 7)
        colors.floating.setFill()
        path.fill()
        colors.control.setStroke()
        path.stroke()
        screenshotSymbol(document.kind.symbol, color: colors.ink)?.draw(in: CGRect(x: label.minX + 10, y: label.minY + 6, width: 16, height: 16),
                                                                       from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
        title.draw(at: CGPoint(x: label.minX + 33, y: label.midY - title.size().height / 2))
    }

    private func outline(_ rect: CGRect, dashed: Bool, in context: CGContext) {
        context.saveGState()
        context.setStrokeColor(NSColor.white.cgColor)
        context.setLineWidth(2)
        context.stroke(rect)
        if dashed { context.setLineDash(phase: 0, lengths: [5, 4]) }
        context.setStrokeColor(ScreenshotRenderer.ink.cgColor)
        context.setLineWidth(1)
        context.stroke(rect)
        context.restoreGState()
    }

    private func drawHandle(_ handle: Handle, at point: CGPoint, in context: CGContext) {
        let pressed = gesture?.handle == handle
        let side: CGFloat = pressed || hoverHandle == handle ? 18 : 14
        var rect = CGRect(x: point.x - side / 2, y: point.y - side / 2, width: side, height: side)
        let path: CGPath
        switch handle {
        case .start, .end: path = CGPath(ellipseIn: rect, transform: nil)
        case .edge(let i):
            rect = rect.insetBy(dx: i.isMultiple(of: 2) ? 0 : 4, dy: i.isMultiple(of: 2) ? 4 : 0)
            path = CGPath(rect: rect, transform: nil)
        case .pointer:
            let diamond = CGMutablePath()
            diamond.move(to: CGPoint(x: rect.midX, y: rect.minY))
            diamond.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
            diamond.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
            diamond.addLine(to: CGPoint(x: rect.minX, y: rect.midY))
            diamond.closeSubpath()
            path = diamond
        default: path = CGPath(rect: rect, transform: nil)
        }
        context.setFillColor((handle == .pointer ? ScreenshotInk.yellow.color : .white).cgColor)
        context.setStrokeColor(ScreenshotRenderer.ink.cgColor)
        context.setLineWidth(2)
        context.addPath(path)
        context.drawPath(using: .fillStroke)
        if pressed {
            let center = CGRect(x: point.x - 3, y: point.y - 3, width: 6, height: 6)
            context.setFillColor(ScreenshotRenderer.ink.cgColor)
            context.fillEllipse(in: center)
        }
    }

    private func source(_ event: NSEvent) -> CGPoint {
        let point = convert(event.locationInWindow, from: nil)
        return CGPoint(x: ((point.x - imageOrigin.x) / factor).rounded(),
                       y: ((point.y - imageOrigin.y) / factor).rounded())
    }

    private func bounded(_ point: CGPoint, within rect: CGRect) -> CGPoint {
        CGPoint(x: min(rect.maxX, max(rect.minX, point.x)), y: min(rect.maxY, max(rect.minY, point.y)))
    }

    private func viewPoint(_ point: CGPoint) -> CGPoint {
        CGPoint(x: imageOrigin.x + point.x * factor, y: imageOrigin.y + point.y * factor)
    }

    private func viewRect(_ rect: CGRect) -> CGRect {
        CGRect(origin: viewPoint(rect.origin), size: CGSize(width: rect.width * factor, height: rect.height * factor))
    }
}


@MainActor
private final class ScreenshotCalloutField: NSTextView {
    var finish: (() -> Void)?

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 || ([36, 76].contains(event.keyCode) &&
                                  !event.modifierFlags.contains(.shift) && !hasMarkedText()) {
            finish?()
        } else {
            super.keyDown(with: event)
        }
    }
}
