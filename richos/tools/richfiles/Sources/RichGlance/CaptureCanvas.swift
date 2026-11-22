import AppKit
import RichFilesCore

@MainActor
final class CaptureCanvas: NSView {
    private static let lensSize: CGFloat = 168
    private static let rim: CGFloat = 2
    private static let cell: CGFloat = 9
    private static let clearance: CGFloat = 9
    private static let margin: CGFloat = 6
    private static let darkStroke = NSColor(srgbRed: 12 / 255, green: 19 / 255, blue: 34 / 255, alpha: 1)
    private static let lightStroke = NSColor(srgbRed: 253 / 255, green: 252 / 255, blue: 248 / 255, alpha: 1)
    private static let guide = NSColor(srgbRed: 1, green: 193 / 255, blue: 26 / 255, alpha: 1)
    private static let guideEdge = NSColor(srgbRed: 56 / 255, green: 30 / 255, blue: 4 / 255, alpha: 1)
    private static let readoutSurface = NSColor(white: 0.14, alpha: 0.97)
    private static let gold = NSColor(srgbRed: 194 / 255, green: 163 / 255, blue: 92 / 255, alpha: 1)
    private static let blankCursor = NSCursor(image: NSImage(size: CGSize(width: 8, height: 8), flipped: false) { _ in true }, hotSpot: .zero)
    private var dark: Bool { effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua }
    private var surface: NSColor {
        dark ? NSColor(srgbRed: 0.094, green: 0.141, blue: 0.251, alpha: 1)
            : NSColor(srgbRed: 0.992, green: 0.988, blue: 0.973, alpha: 1)
    }
    private var ink: NSColor {
        dark ? NSColor(srgbRed: 0.875, green: 0.894, blue: 0.933, alpha: 1) : Self.darkStroke
    }
    private var border: NSColor {
        dark ? NSColor(srgbRed: 147 / 255, green: 153 / 255, blue: 165 / 255, alpha: 1)
            : NSColor(srgbRed: 83 / 255, green: 87 / 255, blue: 94 / 255, alpha: 1)
    }
    private var lensGround: NSColor {
        dark ? NSColor(srgbRed: 6 / 255, green: 10 / 255, blue: 20 / 255, alpha: 1)
            : NSColor(srgbRed: 220 / 255, green: 215 / 255, blue: 203 / 255, alpha: 1)
    }
    let image: CGImage
    private(set) var selection: GlanceCaptureSelection
    private var fixedRepeat = false
    var copiesText = false
    var copy: (() -> Void)?
    var edit: (() -> Void)?
    var save: (() -> Void)?
    var scroll: ((Int) -> Void)? { didSet { scrollMenu.isHidden = scroll == nil } }
    var escape: (() -> Void)?
    var cancel: (() -> Void)?
    var chooseTarget: ((CGPoint?) -> Void)?
    var nextTarget: (() -> Void)?
    var aim: ((CGPoint) -> Void)?
    var changeMode: ((CaptureMode) -> Void)?
    var changeDelay: ((Int) -> Void)?
    var changePointer: ((Bool) -> Void)?
    var changeAction: ((ScreenshotStore.Action) -> Void)?
    private(set) var mode = CaptureMode.region
    private(set) var isActiveDisplay = true
    private(set) var locked = false
    private var targetRect: CGRect?
    private var targetDescription = ""
    private var originTitle = ""
    private var doesIncludePointer = false
    private var delay = 0
    private var returnAction = ScreenshotStore.Action.copy
    private let controls = CaptureControls()
    private let actions = NSStackView()
    private let actionButtons = NSStackView()
    private let hint = NSTextField(labelWithString: "")
    private let copyButton = NSButton(title: "Copy Image", target: nil, action: nil)
    private let editButton = NSButton(title: "Open in Editor", target: nil, action: nil)
    private let saveButton = NSButton(title: "Save PNG…", target: nil, action: nil)
    private let retryButton = NSButton(title: "Back", target: nil, action: nil)
    private let scrollMenu = NSPopUpButton(frame: .zero, pullsDown: true)
    private let actionMenu = NSPopUpButton(frame: .zero, pullsDown: true)
    private var pointerTracking: NSTrackingArea?
    private var regionDrag: (anchor: CGPoint, initial: GlanceCaptureSelection, handle: Int)?
    private var failure: String?
    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    private var geometry: GlanceCaptureGeometry {
        GlanceCaptureGeometry(frame: CGSize(width: image.width, height: image.height), view: bounds.size)
    }

    init(image: CGImage, frame: CGRect, pointer: CGPoint) {
        self.image = image
        selection = GlanceCaptureSelection(width: image.width, height: image.height, pointer: pointer)
        super.init(frame: frame)
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel("Region selection")
        actions.orientation = .vertical
        actions.alignment = .leading
        actions.spacing = 5
        actions.edgeInsets = NSEdgeInsets(top: 7, left: 8, bottom: 7, right: 8)
        actions.wantsLayer = true
        actions.layer?.cornerRadius = 12
        actions.layer?.borderWidth = 1
        hint.font = .systemFont(ofSize: 16)
        hint.lineBreakMode = .byTruncatingTail
        hint.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        actions.addArrangedSubview(hint)
        actionButtons.spacing = 6
        actions.addArrangedSubview(actionButtons)
        for (button, action) in [(copyButton, #selector(copyPressed)), (editButton, #selector(editPressed)),
                                 (saveButton, #selector(savePressed)), (retryButton, #selector(startOver))] {
            button.target = self
            button.action = action
            button.bezelStyle = .rounded
            button.font = .systemFont(ofSize: 16, weight: .medium)
            button.controlSize = .large
            button.heightAnchor.constraint(equalToConstant: 36).isActive = true
        }
        copyButton.setAccessibilityLabel("Copy Image")
        editButton.setAccessibilityLabel("Open in Editor")
        actionMenu.addItems(withTitles: ["↩", "Return copies the image", "Return opens the editor"])
        actionMenu.setAccessibilityLabel("Default capture action")
        actionMenu.target = self
        actionMenu.action = #selector(pickAction)
        scrollMenu.addItems(withTitles: ["Scrolling Capture…", "Scroll Manually", "Auto Scroll Down",
                                         "Auto Scroll Right", "Auto Scroll Down and Right"])
        scrollMenu.setAccessibilityLabel("Scrolling Capture…")
        scrollMenu.target = self
        scrollMenu.action = #selector(pickScroll)
        scrollMenu.isHidden = true
        for menu in [actionMenu, scrollMenu] {
            menu.font = .systemFont(ofSize: 16)
            menu.controlSize = .large
        }
        for view in [copyButton, editButton, actionMenu, saveButton, scrollMenu, retryButton] {
            actionButtons.addArrangedSubview(view)
        }
        controls.modeChanged = { [weak self] in self?.changeMode?($0) }
        controls.delayChanged = { [weak self] in self?.changeDelay?($0) }
        controls.pointerChanged = { [weak self] in self?.changePointer?($0) }
        controls.canceled = { [weak self] in self?.cancel?() }
        addSubview(controls)
        addSubview(actions)
        applyAppearance()
        refresh()
    }

    required init?(coder: NSCoder) { nil }

    var ready: Bool { mode == .region ? selection.phase == .refining && selection.canCapture : locked }
    var visibleCrop: CGRect? { mode == .region ? selection.crop : targetRect.map(geometry.sourceRect) }

    func configure(mode: CaptureMode, active: Bool, delay: Int, pointer: Bool, action: ScreenshotStore.Action, origin: String) {
        self.mode = mode
        isActiveDisplay = active
        self.delay = delay
        doesIncludePointer = pointer
        returnAction = action
        originTitle = origin
        controls.update(mode: mode, delay: delay, includesPointer: pointer)
        controls.isHidden = !active || copiesText
        editButton.isHidden = copiesText
        saveButton.isHidden = copiesText
        actionMenu.isHidden = copiesText
        scrollMenu.isHidden = copiesText || scroll == nil
        copyButton.title = copiesText ? "Copy Text ↩" : action == .copy ? "Copy Image ↩" : "Copy Image"
        copyButton.setAccessibilityLabel(copiesText ? "Copy Text" : "Copy Image")
        retryButton.title = copiesText ? "Adjust Area" : "Back"
        editButton.title = action == .editor ? "Open in Editor ↩" : "Open in Editor"
        actionMenu.item(at: 1)?.state = action == .copy ? .on : .off
        actionMenu.item(at: 2)?.state = action == .editor ? .on : .off
        applyAppearance()
        refresh()
    }

    func prefillRepeat(_ regionSize: CGSize) {
        let width = min(CGFloat(image.width), regionSize.width)
        let height = min(CGFloat(image.height), regionSize.height)
        let origin = CGPoint(x: (CGFloat(image.width) - width) / 2, y: (CGFloat(image.height) - height) / 2)
        selection.restore(CGRect(origin: origin, size: CGSize(width: width, height: height)))
        fixedRepeat = width == regionSize.width && height == regionSize.height
        refresh()
    }

    func updateTarget(_ rect: CGRect?, description: String, locked: Bool) {
        targetRect = rect
        targetDescription = description
        self.locked = locked
        refresh()
    }

    func focusAction() {
        let action: NSView = returnAction == .copy ? copyButton : editButton
        window?.makeFirstResponder(ready ? action : self)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let pointerTracking { removeTrackingArea(pointerTracking) }
        let tracking = NSTrackingArea(rect: .zero, options: [.mouseMoved, .inVisibleRect, .activeInActiveApp],
                                      owner: self, userInfo: nil)
        addTrackingArea(tracking)
        pointerTracking = tracking
    }

    override func resetCursorRects() {
        let usesGuides = mode == .region && isActiveDisplay && selection.phase != .refining
        addCursorRect(bounds, cursor: usesGuides ? Self.blankCursor : .arrow)
        if usesGuides, !controls.isHidden { addCursorRect(controls.frame, cursor: .arrow) }
    }

    override func mouseMoved(with event: NSEvent) {
        aim?(event.locationInWindow)
        if isActiveDisplay, mode == .region { movePointer(event) }
    }

    override func mouseDown(with event: NSEvent) {
        aim?(event.locationInWindow)
        guard isActiveDisplay else { return }
        if mode != .region { chooseTarget?(event.locationInWindow); return }
        if selection.phase == .refining, let box = selection.crop {
            let local = convert(event.locationInWindow, from: nil)
            let handles = positions(geometry.viewRect(box))
            if !fixedRepeat, let index = handles.firstIndex(where: { hypot($0.x - local.x, $0.y - local.y) <= 10 }) {
                regionDrag = (source(event), selection, index)
            } else if geometry.viewRect(box).contains(local) {
                regionDrag = (source(event), selection, 8)
            }
        } else if selection.phase == .placing {
            selection.movePointer(to: source(event))
            selection.shiftReturn()
        } else {
            selection.mouseDown(at: source(event))
        }
        refresh()
    }

    override func mouseDragged(with event: NSEvent) {
        guard mode == .region else { return }
        guard let drag = regionDrag, let box = drag.initial.crop else { movePointer(event); return }
        let point = source(event)
        let dx = (point.x - drag.anchor.x).rounded()
        let dy = (point.y - drag.anchor.y).rounded()
        selection = drag.initial
        if drag.handle == 8 {
            selection.translate(dx: Int(dx), dy: Int(dy))
        } else {
            let starting = positions(box)[drag.handle]
            let movedX = min(CGFloat(image.width), max(0, starting.x + dx))
            let movedY = min(CGFloat(image.height), max(0, starting.y + dy))
            let active = drag.initial.target
            if drag.handle < 4 {
                let fixed = positions(box)[(drag.handle + 2) % 4]
                selection.refine(active: CGPoint(x: movedX, y: movedY), fixed: fixed)
            } else {
                let fixed = drag.initial.active == .first ? drag.initial.second : drag.initial.first
                var a = CGPoint(x: active.x, y: active.y)
                var b = CGPoint(x: fixed.x, y: fixed.y)
                switch drag.handle {
                case 4: if a.y <= b.y { a.y = min(movedY, b.y - 1) } else { b.y = min(movedY, a.y - 1) }
                case 5: if a.x >= b.x { a.x = max(movedX, b.x + 1) } else { b.x = max(movedX, a.x + 1) }
                case 6: if a.y >= b.y { a.y = max(movedY, b.y + 1) } else { b.y = max(movedY, a.y + 1) }
                default: if a.x <= b.x { a.x = min(movedX, b.x - 1) } else { b.x = min(movedX, a.x - 1) }
                }
                selection.refine(active: a, fixed: b)
            }
        }
        refresh()
    }

    override func mouseUp(with event: NSEvent) {
        guard mode == .region else { return }
        regionDrag = nil
        selection.mouseUp()
        refresh()
        focusAction()
    }

    override func keyDown(with event: NSEvent) {
        if !handleKey(event) { super.keyDown(with: event) }
    }

    func handleKey(_ event: NSEvent) -> Bool {
        let modifiers = event.modifierFlags.intersection([.shift, .command, .control, .option])
        let key = event.charactersIgnoringModifiers?.lowercased() ?? ""
        defer { needsDisplay = true }
        if modifiers == [.shift], event.keyCode == 36, mode == .region {
            if !event.isARepeat { selection.shiftReturn(); refresh(); focusAction() }
        } else if !copiesText, modifiers == [.command], key == "s" {
            if !event.isARepeat { save?() }
        } else if modifiers.isEmpty || modifiers == .option {
            let direction: [UInt16: (Int, Int)] = [123: (-1, 0), 124: (1, 0), 125: (0, 1), 126: (0, -1)]
            if let move = direction[event.keyCode], mode == .region {
                if modifiers == .option || fixedRepeat { selection.translate(dx: move.0, dy: move.1) }
                else { selection.nudge(dx: move.0, dy: move.1) }
                refresh()
                return true
            }
            guard modifiers.isEmpty else { return false }
            switch event.keyCode {
            case 48:
                if !event.isARepeat {
                    if mode == .region { selection.switchCorner(); refresh() }
                    else { nextTarget?() }
                }
            case 53: if !event.isARepeat { escape?() }
            case 36, 76:
                if !event.isARepeat {
                    if mode != .region, !ready { chooseTarget?(nil) }
                    else { performReturn() }
                }
            case 49: if !copiesText, !event.isARepeat { changeMode?(mode.next) }
            default:
                guard ["m", "r", "t", "p", "c", "e", "s"].contains(key) else { return false }
                guard !event.isARepeat else { return true }
                if copiesText, !["m", "r", "c"].contains(key) { return true }
                switch key {
                case "m": if mode == .region { selection.toggleLens(); refresh() }
                case "r": startOver()
                case "t":
                    let values = [0, 3, 5, 10]
                    changeDelay?(values[((values.firstIndex(of: delay) ?? 0) + 1) % values.count])
                case "p": changePointer?(!doesIncludePointer)
                case "c": copy?()
                case "e": edit?()
                default: if scroll != nil, ready { scrollMenu.performClick(nil) }
                }
            }
        } else { return false }
        return true
    }

    private func performReturn() {
        if returnAction == .copy { copy?() }
        else { edit?() }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyAppearance()
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        guard isActiveDisplay || targetRect != nil else { return }
        if mode != .region {
            if let targetRect { drawTarget(targetRect, context: context) }
            drawFailure()
            return
        }
        let target = geometry.viewPoint(selection.target)
        if let crop = selection.crop, !crop.isEmpty {
            let box = geometry.viewRect(crop)
            let shade = CGMutablePath()
            shade.addRect(bounds)
            shade.addRect(box)
            context.setFillColor(Self.darkStroke.withAlphaComponent(0.5).cgColor)
            context.addPath(shade)
            context.drawPath(using: .eoFill)
            let line = 1 / (window?.backingScaleFactor ?? 1)
            for start in stride(from: box.minY, to: box.maxY, by: 14) {
                for x in [box.minX, box.maxX] { vertical(x, start, min(start + 9, box.maxY), in: context, thickness: line) }
            }
            for start in stride(from: box.minX, to: box.maxX, by: 14) {
                for y in [box.minY, box.maxY] { horizontal(y, start, min(start + 9, box.maxX), in: context, thickness: line) }
            }
        }
        crosshair(at: target, context: context)
        if selection.phase != .aiming {
            drawCorners(at: target, context: context)
            if selection.phase == .refining, let crop = selection.crop { drawHandles(geometry.viewRect(crop), context: context) }
        }
        let position = lensPlacement(around: target)
        label(readout, in: position.label, size: 16, compact: true)
        if selection.showsLens { drawLens(in: position.lens, context: context) }
        drawFailure()
    }

    private func drawFailure() {
        if let failure {
            let width = min(bounds.width - 2 * Self.margin, 675)
            label(failure, in: CGRect(x: (bounds.width - width) / 2, y: Self.margin, width: width, height: 34), size: 16)
        }
    }

    func showError(_ message: String) {
        failure = message
        setAccessibilityValue(message)
        needsDisplay = true
    }

    @objc private func copyPressed() { copy?() }
    @objc private func editPressed() { edit?() }
    @objc private func savePressed() { save?() }
    @objc private func startOver() { if ready { escape?() } }
    @objc private func pickAction() {
        if actionMenu.indexOfSelectedItem > 0 { changeAction?(actionMenu.indexOfSelectedItem == 1 ? .copy : .editor) }
    }
    @objc private func pickScroll() {
        if scrollMenu.indexOfSelectedItem > 0 { scroll?(scrollMenu.indexOfSelectedItem - 1) }
    }

    func resetSelection() -> Bool {
        fixedRepeat = false
        guard let window else { return false }
        let point = convert(window.mouseLocationOutsideOfEventStream, from: nil)
        guard selection.discardSelection(at: geometry.sourcePoint(point)) else { return false }
        refresh()
        window.makeFirstResponder(self)
        return true
    }

    private func source(_ event: NSEvent) -> CGPoint {
        geometry.sourcePoint(convert(event.locationInWindow, from: nil))
    }

    private func movePointer(_ event: NSEvent) {
        let before = selection.target
        selection.movePointer(to: source(event))
        if before != selection.target { refresh() }
    }

    private var dimensions: String {
        "\(selection.crop == nil ? "Display" : "Crop") \(readout)"
    }

    private var readout: String {
        guard let crop = selection.crop else { return "\(image.width) × \(image.height) px" }
        return "\(Int(crop.width)) × \(Int(crop.height)) px"
    }

    private func refresh() {
        failure = nil
        setAccessibilityValue(mode == .region ? dimensions : targetDescription)
        window?.invalidateCursorRects(for: self)
        actions.isHidden = !isActiveDisplay || !ready
        let area = mode == .region ? selection.crop.map(geometry.viewRect) : targetRect
        if !actions.isHidden, let area {
            let placement = mode == .region ? lensPlacement(around: geometry.viewPoint(selection.target)) : (.zero, .zero)
            let size = mode == .region ? readout : targetDescription
            let verb = returnAction == .copy ? "copies" : "opens"
            let suffix = delay == 0 ? "" : " after \(delay) s"
            let route = returnAction == .copy && !originTitle.isEmpty ? " and takes you back to \(originTitle)" : ""
            hint.stringValue = copiesText ? "Return copies text from this area." : "Return \(verb) \(size)\(suffix)\(route)."
            actions.frame = actionPlacement(around: area, avoiding: placement)
        }
        let controlsSize = controls.fittingSize
        let bottom = CGRect(x: (bounds.width - controlsSize.width) / 2,
                            y: bounds.maxY - controlsSize.height - 14, width: controlsSize.width, height: controlsSize.height)
        let point = mode == .region ? geometry.viewPoint(selection.target) : .zero
        // Keep the bar reachable from above. Move it when aiming below it or when it covers a crop.
        let aimsBelow = point.y > bottom.maxY && (bottom.minX...bottom.maxX).contains(point.x)
        let top = CGRect(x: bottom.minX, y: 14, width: bottom.width, height: bottom.height)
        let pointsAtTopBar = controls.frame == top && top.contains(point)
        controls.frame = area?.intersects(bottom) == true || aimsBelow || pointsAtTopBar ||
            (!actions.isHidden && actions.frame.intersects(bottom)) ? top : bottom
        needsDisplay = true
    }

    private func applyAppearance() {
        actions.layer?.backgroundColor = surface.cgColor
        actions.layer?.borderColor = border.cgColor
        hint.textColor = ink
        styleAction(copyButton, primary: returnAction == .copy)
        styleAction(editButton, primary: returnAction == .editor)
        styleAction(saveButton, primary: false)
        styleAction(retryButton, primary: false)
    }

    private func positions(_ box: CGRect) -> [CGPoint] {
        [CGPoint(x: box.minX, y: box.minY), CGPoint(x: box.maxX, y: box.minY),
         CGPoint(x: box.maxX, y: box.maxY), CGPoint(x: box.minX, y: box.maxY),
         CGPoint(x: box.midX, y: box.minY), CGPoint(x: box.maxX, y: box.midY),
         CGPoint(x: box.midX, y: box.maxY), CGPoint(x: box.minX, y: box.midY)]
    }

    private func drawHandles(_ box: CGRect, context: CGContext) {
        let a = geometry.viewPoint(selection.first)
        let b = geometry.viewPoint(selection.second)
        for (index, point) in positions(box).enumerated() where point != a && point != b {
            let width: CGFloat = index < 4 ? 12 : index.isMultiple(of: 2) ? 22 : 8
            let height: CGFloat = index < 4 ? 12 : index.isMultiple(of: 2) ? 8 : 22
            let handle = CGRect(x: point.x - width / 2, y: point.y - height / 2, width: width, height: height)
            context.setFillColor(Self.lightStroke.cgColor)
            context.fill(handle)
            context.setStrokeColor(Self.darkStroke.cgColor)
            context.setLineWidth(2)
            context.stroke(handle)
        }
    }

    private func drawTarget(_ rect: CGRect, context: CGContext) {
        context.saveGState()
        let box = rect.intersection(bounds).insetBy(dx: 2, dy: 2)
        defer { context.restoreGState() }
        guard box.width > 0, box.height > 0 else { return }
        if locked {
            let shade = CGMutablePath()
            shade.addRect(bounds)
            shade.addRect(box)
            context.addPath(shade)
            context.setFillColor(Self.darkStroke.withAlphaComponent(0.50).cgColor)
            context.drawPath(using: .eoFill)
        } else {
            context.setLineDash(phase: 0, lengths: [8, 5])
        }
        context.setLineWidth(3)
        context.setStrokeColor(Self.darkStroke.cgColor)
        context.stroke(box)
        context.setLineWidth(1)
        context.setStrokeColor(Self.lightStroke.cgColor)
        context.stroke(box)
        if locked {
            context.setLineWidth(4)
            context.setStrokeColor(Self.gold.cgColor)
            for (i, point) in positions(box).prefix(4).enumerated() {
                let x: CGFloat = i == 0 || i == 3 ? 14 : -14
                let y: CGFloat = i < 2 ? 14 : -14
                context.move(to: CGPoint(x: point.x + x, y: point.y))
                context.addLine(to: point)
                context.addLine(to: CGPoint(x: point.x, y: point.y + y))
                context.strokePath()
            }
        }
        let width = min(450, bounds.width - 16)
        let labelX = min(bounds.maxX - width - 8, max(8, box.midX - width / 2))
        label(targetDescription, in: CGRect(x: labelX, y: box.minY > 30 ? box.minY - 30 : box.minY + 8,
                                           width: width, height: 26), size: 16)
    }

    private func styleAction(_ button: NSButton, primary: Bool) {
        let signal = dark ? Self.gold : NSColor(srgbRed: 113 / 255, green: 87 / 255, blue: 21 / 255, alpha: 1)
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 8
        button.layer?.borderWidth = 1
        button.layer?.borderColor = (primary ? signal : border).cgColor
        button.layer?.backgroundColor = (primary ? signal : surface).cgColor
        button.attributedTitle = NSAttributedString(string: "  " + button.title.trimmingCharacters(in: .whitespaces) + "  ", attributes: [
            .font: NSFont.systemFont(ofSize: 16, weight: primary ? .semibold : .medium),
            .foregroundColor: primary ? (dark ? Self.darkStroke : NSColor.white) : ink
        ])
    }

    private func lensPlacement(around center: CGPoint) -> (lens: CGRect, label: CGRect) {
        let size = Self.lensSize
        let outside = size + 2 * Self.rim
        let labelWidth: CGFloat = selection.showsLens ? outside : 184
        let labelHeight: CGFloat = 26
        let usable = bounds.insetBy(dx: Self.margin, dy: Self.margin)
        let quadrants: [(CGFloat, CGFloat)] = [(-1, 1), (1, 1), (-1, -1), (1, -1)]
        let crop = selection.crop.map(geometry.viewRect)
        var firstFit: (CGRect, CGRect)?
        for (x, y) in quadrants {
            let offset = outside / 2 + Self.clearance
            let position = CGPoint(x: center.x + x * offset, y: center.y + y * offset)
            let outer = CGRect(x: position.x - outside / 2, y: position.y - outside / 2, width: outside, height: outside)
            let pillX = min(max(usable.minX, position.x - labelWidth / 2), usable.maxX - labelWidth)
            let pillY = y > 0 ? outer.maxY - 4 : outer.minY + 4 - labelHeight
            let pill = selection.showsLens
                ? CGRect(x: pillX, y: pillY, width: labelWidth, height: labelHeight)
                : CGRect(x: x > 0 ? center.x + 16 : center.x - 16 - labelWidth,
                         y: y > 0 ? center.y + 16 : center.y - 16 - labelHeight, width: labelWidth, height: labelHeight)
            if usable.contains(pill) && (!selection.showsLens || usable.contains(outer)) {
                let placement = (outer.insetBy(dx: Self.rim, dy: Self.rim), pill)
                let occupied = selection.showsLens ? outer.union(pill) : pill
                if crop?.intersects(occupied) != true { return placement }
                if firstFit == nil { firstFit = placement }
            }
        }
        if let firstFit { return firstFit }
        let lens = CGRect(x: usable.midX - size / 2, y: usable.minY + Self.rim, width: size, height: size)
        return (lens, CGRect(x: usable.midX - labelWidth / 2, y: lens.maxY - 2, width: labelWidth, height: labelHeight))
    }

    private func actionPlacement(around box: CGRect, avoiding hud: (lens: CGRect, label: CGRect)) -> CGRect {
        var size = actions.fittingSize
        size.width = min(bounds.width - 2 * Self.margin, max(size.width, actionButtons.fittingSize.width + 16))
        let usable = bounds.insetBy(dx: Self.margin, dy: Self.margin)
        let x = min(max(usable.minX, box.midX - size.width / 2), usable.maxX - size.width)
        let y = min(max(usable.minY, box.midY - size.height / 2), usable.maxY - size.height)
        let gap: CGFloat = 14
        let positions = [
            CGPoint(x: x, y: box.maxY + gap), CGPoint(x: x, y: box.minY - gap - size.height),
            CGPoint(x: box.maxX + gap, y: y), CGPoint(x: box.minX - gap - size.width, y: y),
            CGPoint(x: x, y: box.maxY - gap - size.height),
            CGPoint(x: x, y: y), CGPoint(x: usable.midX - size.width / 2, y: usable.maxY - size.height - 8),
            CGPoint(x: usable.midX - size.width / 2, y: usable.minY + 36)
        ]
        let corners = [selection.first, selection.second].map(geometry.viewPoint).map {
            CGRect(x: $0.x - 26, y: $0.y - 26, width: 52, height: 52)
        }
        let hudArea = (selection.showsLens ? hud.lens.union(hud.label) : hud.label).insetBy(dx: -8, dy: -8)
        return positions.map { CGRect(origin: $0, size: size) }.first {
            usable.contains($0) && !$0.intersects(hudArea) && !corners[0].intersects($0) && !corners[1].intersects($0)
        } ?? CGRect(x: usable.midX - size.width / 2, y: usable.maxY - size.height, width: size.width, height: size.height)
    }

    private func paint(_ bitmap: CGImage, in rect: CGRect, context: CGContext) {
        context.saveGState()
        context.interpolationQuality = .none
        context.translateBy(x: rect.minX, y: rect.maxY)
        context.scaleBy(x: 1, y: -1)
        context.draw(bitmap, in: CGRect(origin: .zero, size: rect.size))
        context.restoreGState()
    }

    private func crosshair(at point: CGPoint, context: CGContext) {
        let line = 1 / (window?.backingScaleFactor ?? 1)
        for start in stride(from: bounds.minY, to: bounds.maxY, by: 14) {
            vertical(point.x, start, min(start + 9, bounds.maxY), in: context, thickness: line)
        }
        for start in stride(from: bounds.minX, to: bounds.maxX, by: 14) {
            horizontal(point.y, start, min(start + 9, bounds.maxX), in: context, thickness: line)
        }
        vertical(point.x, point.y - 5, point.y + 5, in: context, thickness: line)
        horizontal(point.y, point.x - 5, point.x + 5, in: context, thickness: line)
        // The heavier ticks make each arm easy to find while keeping the intersection fine.
        vertical(point.x, point.y - 80, point.y - 33, in: context, thickness: 1.5)
        vertical(point.x, point.y + 33, point.y + 80, in: context, thickness: 1.5)
        horizontal(point.y, point.x - 80, point.x - 33, in: context, thickness: 1.5)
        horizontal(point.y, point.x + 33, point.x + 80, in: context, thickness: 1.5)
    }

    private func vertical(_ x: CGFloat, _ top: CGFloat, _ bottom: CGFloat, in context: CGContext, thickness: CGFloat) {
        let edge = min(thickness, 1 / (window?.backingScaleFactor ?? 1))
        context.setFillColor(Self.guideEdge.withAlphaComponent(0.8).cgColor)
        context.fill(CGRect(x: x - edge, y: top, width: edge, height: bottom - top))
        context.setFillColor(Self.guide.cgColor)
        context.fill(CGRect(x: x, y: top, width: thickness, height: bottom - top))
    }

    private func horizontal(_ y: CGFloat, _ left: CGFloat, _ right: CGFloat, in context: CGContext, thickness: CGFloat) {
        let edge = min(thickness, 1 / (window?.backingScaleFactor ?? 1))
        context.setFillColor(Self.guideEdge.withAlphaComponent(0.8).cgColor)
        context.fill(CGRect(x: left, y: y - edge, width: right - left, height: edge))
        context.setFillColor(Self.guide.cgColor)
        context.fill(CGRect(x: left, y: y, width: right - left, height: thickness))
    }

    private func drawCorners(at target: CGPoint, context: CGContext) {
        let fixed = geometry.viewPoint(selection.active == .first ? selection.second : selection.first)
        let ring = CGRect(x: fixed.x - 6, y: fixed.y - 6, width: 12, height: 12)
        context.setStrokeColor(Self.lightStroke.cgColor)
        context.setLineWidth(5)
        context.strokeEllipse(in: ring)
        context.setStrokeColor(Self.darkStroke.cgColor)
        context.setLineWidth(2.5)
        context.strokeEllipse(in: ring)

        var x: CGFloat = target.x >= fixed.x ? 1 : -1
        var y: CGFloat = target.y >= fixed.y ? 1 : -1
        if !(0...bounds.width).contains(target.x + x * 9) { x *= -1 }
        if !(0...bounds.height).contains(target.y + y * 9) { y *= -1 }
        let corners: [CGPoint] = [CGPoint(x: -18, y: 4), CGPoint(x: 4, y: 4), CGPoint(x: 4, y: -18),
                                  CGPoint(x: 9, y: -18), CGPoint(x: 9, y: 9), CGPoint(x: -18, y: 9)]
        let path = CGMutablePath()
        path.addLines(between: corners.map { CGPoint(x: target.x + $0.x * x, y: target.y + $0.y * y) })
        path.closeSubpath()
        context.addPath(path)
        context.setStrokeColor(Self.lightStroke.cgColor)
        context.setLineWidth(4)
        context.strokePath()
        context.addPath(path)
        context.setStrokeColor(Self.darkStroke.cgColor)
        context.setLineWidth(2)
        context.strokePath()
        context.addPath(path)
        context.setFillColor(Self.guide.cgColor)
        context.fillPath()
    }

    private func drawLens(in rect: CGRect, context: CGContext) {
        let center = CGPoint(x: rect.midX, y: rect.midY)
        let cell = Self.cell
        let count = Int(ceil(rect.width / (2 * cell)))
        let point = selection.target
        let wanted = CGRect(x: point.x - count, y: point.y - count, width: 2 * count, height: 2 * count)
        let available = wanted.intersection(CGRect(x: 0, y: 0, width: image.width, height: image.height))
        context.saveGState()
        context.addEllipse(in: rect)
        context.clip()
        context.setFillColor(lensGround.cgColor)
        context.fill(rect)
        context.setStrokeColor(border.withAlphaComponent(0.5).cgColor)
        context.setLineWidth(1)
        for offset in stride(from: -rect.width, to: rect.width, by: 7) {
            context.move(to: CGPoint(x: rect.minX + offset, y: rect.maxY))
            context.addLine(to: CGPoint(x: rect.minX + offset + rect.width, y: rect.minY))
        }
        context.strokePath()
        if !available.isEmpty, let sample = image.cropping(to: available) {
            paint(sample, in: CGRect(x: center.x + (available.minX - CGFloat(point.x)) * cell,
                                     y: center.y + (available.minY - CGFloat(point.y)) * cell,
                                     width: available.width * cell, height: available.height * cell), context: context)
            drawCellEdges(sample, sourceRect: available, at: center, context: context)
        }
        vertical(center.x, rect.minY, rect.maxY, in: context, thickness: 1.5)
        horizontal(center.y, rect.minX, rect.maxX, in: context, thickness: 1.5)
        context.restoreGState()
        context.setStrokeColor(Self.lightStroke.cgColor)
        context.setLineWidth(Self.rim)
        context.strokeEllipse(in: rect.insetBy(dx: -Self.rim / 2, dy: -Self.rim / 2))
        context.setStrokeColor(Self.readoutSurface.cgColor)
        context.setLineWidth(1)
        context.strokeEllipse(in: rect.insetBy(dx: -Self.rim, dy: -Self.rim))
    }

    private func drawCellEdges(_ sample: CGImage, sourceRect: CGRect, at center: CGPoint, context: CGContext) {
        let width = sample.width
        let height = sample.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let point = selection.target
        let cell = Self.cell
        let line = 1 / (window?.backingScaleFactor ?? 1)
        pixels.withUnsafeMutableBytes { bytes in
            guard let color = CGColorSpace(name: CGColorSpace.sRGB),
                  let bitmap = CGContext(data: bytes.baseAddress, width: width, height: height,
                                         bitsPerComponent: 8, bytesPerRow: width * 4, space: color,
                                         bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return }
            bitmap.draw(sample, in: CGRect(x: 0, y: 0, width: width, height: height))
            for row in 0..<height {
                for column in 0..<width {
                    let index = (row * width + column) * 4
                    let red = Self.linear(bytes[index]), green = Self.linear(bytes[index + 1]), blue = Self.linear(bytes[index + 2])
                    let luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue
                    let darken = luminance >= 0.18
                    let target = darken ? (luminance + 0.05) / 3.05 - 0.05 : (luminance + 0.05) * 3.05 - 0.05
                    let fraction = darken ? target / luminance : (target - luminance) / (1 - luminance)
                    func edge(_ value: Double) -> CGFloat {
                        Self.encoded(darken ? value * fraction : value + (1 - value) * fraction)
                    }
                    let x = center.x + (sourceRect.minX + CGFloat(column - point.x)) * cell
                    let y = center.y + (sourceRect.minY + CGFloat(row - point.y)) * cell
                    context.setFillColor(NSColor(srgbRed: edge(red), green: edge(green), blue: edge(blue), alpha: 1).cgColor)
                    context.fill(CGRect(x: x, y: y + cell - line, width: cell, height: line))
                    context.fill(CGRect(x: x + cell - line, y: y, width: line, height: cell))
                }
            }
        }
    }

    private nonisolated static func linear(_ byte: UInt8) -> Double {
        let component = Double(byte) / 255
        return component <= 0.04045 ? component / 12.92 : pow((component + 0.055) / 1.055, 2.4)
        }
    private nonisolated static func encoded(_ value: Double) -> CGFloat {
        value <= 0.0031308 ? 12.92 * value : 1.055 * pow(value, 1 / 2.4) - 0.055
        }

    private func label(_ title: String, in rect: CGRect, size: CGFloat, compact: Bool = false) {
        let radius: CGFloat = compact ? 6 : 17
        (compact ? Self.readoutSurface : surface).setFill()
        let pill = NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)
        pill.fill()
        (compact ? Self.guideEdge : border).setStroke()
        pill.lineWidth = 1
        pill.stroke()
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        let text = title as NSString
        let font = NSFont.monospacedDigitSystemFont(ofSize: size, weight: .semibold)
        let attributes: [NSAttributedString.Key: Any] = [
            .foregroundColor: compact ? NSColor.white : ink, .font: font, .paragraphStyle: paragraph
        ]
        let height = text.size(withAttributes: attributes).height
        text.draw(in: CGRect(x: rect.minX + 4, y: rect.midY - height / 2, width: rect.width - 8, height: height), withAttributes: attributes)
    }
}
