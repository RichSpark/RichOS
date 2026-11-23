import AppKit

@MainActor
struct ScreenshotEditorPalette {
    let chrome, bar, ground, shelf, ink, muted, floating, line, control, gold, onGold, hover: NSColor

    init(_ view: NSView) {
        let dark = view.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        func color(_ light: Int, _ night: Int) -> NSColor {
            let rgb = dark ? night : light
            return NSColor(srgbRed: CGFloat(rgb >> 16) / 255, green: CGFloat((rgb >> 8) & 255) / 255,
                           blue: CGFloat(rgb & 255) / 255, alpha: 1)
        }
        chrome = color(0xeae6dd, 0x0c1322)
        bar = color(0xf3f0e9, 0x111a2e)
        ground = color(0xd3cdbf, 0x060a14)
        shelf = color(0xe1dcd0, 0x0a101b)
        ink = color(0x0c1322, 0xdfe4ee)
        muted = color(0x4d5158, 0xa8aeba)
        floating = color(0xfdfcf8, 0x182440)
        line = color(0xc4bfb3, 0x2a3654)
        control = color(0x767c8d, 0x6b7ea8)
        gold = color(0x715715, 0xc2a35c)
        onGold = color(0xffffff, 0x0c1322)
        hover = color(0xddd5c0, 0x22304f)
    }
}

@MainActor
final class ScreenshotSurface: NSView {
    enum Tone { case chrome, bar, ground, shelf, floating }
    var changedAppearance: (() -> Void)?
    private let tone: Tone
    private let radius: CGFloat
    private let outline: Bool

    init(_ tone: Tone, radius: CGFloat = 0, outline: Bool = false) {
        self.tone = tone
        self.radius = radius
        self.outline = outline
        super.init(frame: .zero)
        wantsLayer = true
        updateColors()
    }

    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
        changedAppearance?()
    }

    private func updateColors() {
        let p = ScreenshotEditorPalette(self)
        let color: NSColor
        switch tone {
        case .chrome: color = p.chrome
        case .bar: color = p.bar
        case .ground: color = p.ground
        case .shelf: color = p.shelf
        case .floating: color = p.floating
        }
        layer?.backgroundColor = color.cgColor
        layer?.cornerRadius = radius
        layer?.borderWidth = outline ? 1 : 0
        layer?.borderColor = p.control.cgColor
    }
}

@MainActor
final class ScreenshotButton: NSButton {
    var symbol: String? { didSet { needsDisplay = true } }
    var keyHint: String? { didSet { needsDisplay = true } }
    var counter: Int? { didSet { needsDisplay = true } }
    var trailingSymbol: String? { didSet { needsDisplay = true } }
    var iconOnly = false { didSet { needsDisplay = true } }
    var isSelected = false {
        didSet {
            needsDisplay = true
            if style == .tool || style == .segment || style == .swatch {
                setAccessibilityValue(isSelected ? "Selected" : "")
            }
        }
    }
    var style: Style = .outline
    var swatch: NSColor?
    var sampleWidth: CGFloat?
    var onHover: ((Bool) -> Void)?
    var onPress: (() -> Void)?
    var onArrow: ((Int) -> Void)?
    private var hover = false
    private var tracking: NSTrackingArea?

    enum Style { case outline, plain, tool, primary, segment, swatch, deleteCapture }

    init(_ text: String, symbol: String? = nil, hint: String? = nil, style: Style = .outline) {
        super.init(frame: .zero)
        title = text
        self.symbol = symbol
        keyHint = hint
        self.style = style
        isBordered = false
        focusRingType = .none
        font = .systemFont(ofSize: 16, weight: .medium)
        target = self
        action = #selector(pressed)
        setAccessibilityLabel(text)
    }

    required init?(coder: NSCoder) { nil }

    @objc private func pressed() { onPress?() }

    override var isEnabled: Bool { didSet { needsDisplay = true } }
    override var acceptsFirstResponder: Bool { isEnabled }
    override func viewDidChangeEffectiveAppearance() { super.viewDidChangeEffectiveAppearance(); needsDisplay = true }
    override func becomeFirstResponder() -> Bool { needsDisplay = true; return super.becomeFirstResponder() }
    override func resignFirstResponder() -> Bool { needsDisplay = true; return super.resignFirstResponder() }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: .zero, options: [.activeInKeyWindow, .mouseEnteredAndExited, .inVisibleRect],
                                  owner: self, userInfo: nil)
        addTrackingArea(area)
        tracking = area
    }

    override func mouseEntered(with event: NSEvent) { hover = true; onHover?(true); needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; onHover?(false); needsDisplay = true }

    override func keyDown(with event: NSEvent) {
        if let onArrow, [123, 124].contains(event.keyCode) {
            onArrow(event.keyCode == 123 ? -1 : 1)
        } else if [36, 76].contains(event.keyCode), isEnabled {
            performClick(nil)
        } else {
            super.keyDown(with: event)
        }
    }

    func naturalWidth() -> CGFloat {
        if style == .swatch { return 34 }
        if iconOnly { return 38 }
        let font = NSFont.systemFont(ofSize: 16, weight: .semibold)
        let text = (title as NSString).size(withAttributes: [.font: font]).width
        let key = keyHint.map { ($0 as NSString).size(withAttributes: [.font: font]).width + 21 } ?? 0
        let decorations: CGFloat = (counter == nil ? 0 : 31) + (trailingSymbol == nil ? 0 : 16)
        return ceil(text + key + decorations + (symbol == nil ? 0 : 25) + 24)
    }

    override func draw(_ dirtyRect: NSRect) {
        let p = ScreenshotEditorPalette(self)
        NSGraphicsContext.saveGraphicsState()
        defer { NSGraphicsContext.restoreGraphicsState() }
        if !isEnabled { NSGraphicsContext.current?.cgContext.setAlpha(0.38) }
        let selected = isSelected || style == .primary
        let radius: CGFloat = style == .swatch || style == .deleteCapture ? bounds.height / 2 : 9
        let path = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: radius, yRadius: radius)
        if style == .deleteCapture || selected && style != .swatch || (hover && isEnabled) {
            (style == .deleteCapture ? p.ink : selected && style != .swatch ? p.gold : p.hover).setFill()
            path.fill()
        }
        if style == .outline || window?.firstResponder === self || (style == .swatch && isSelected) {
            let focused = window?.firstResponder === self
            let ring = selected && style != .swatch ? p.onGold : p.gold
            (focused ? ring : style == .swatch ? p.ink : p.control).setStroke()
            path.lineWidth = focused ? 2 : 1
            path.stroke()
        }
        if let swatch {
            let dot = NSBezierPath(ovalIn: bounds.insetBy(dx: 6, dy: 6))
            swatch.setFill(); dot.fill()
            p.control.setStroke(); dot.stroke()
            return
        }

        let ink = style == .deleteCapture ? p.floating : selected ? p.onGold : p.ink
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 16, weight: selected ? .semibold : .medium),
            .foregroundColor: ink
        ]
        let label = NSAttributedString(string: iconOnly ? "" : title, attributes: attributes)
        let key = keyHint.map { NSAttributedString(string: $0, attributes: attributes) }
        if let sampleWidth {
            ink.setFill()
            let sample = NSRect(x: 12, y: bounds.midY - sampleWidth / 2, width: 98, height: sampleWidth)
            let radius = sampleWidth >= 4 ? sampleWidth / 2 : 0
            NSBezierPath(roundedRect: sample, xRadius: radius, yRadius: radius).fill()
            label.draw(at: CGPoint(x: 124, y: bounds.midY - label.size().height / 2))
            if selected {
                draw(screenshotSymbol("checkmark", color: ink),
                     in: CGRect(x: bounds.maxX - 29, y: bounds.midY - 8, width: 16, height: 16))
            }
            return
        }
        let glyph = symbol.flatMap { screenshotSymbol($0, color: ink, size: style == .tool ? 22 : 17) }
        if style == .tool {
            let glyphSize = min(22, bounds.height - 20)
            let top = bounds.midY - (glyphSize + label.size().height + 2) / 2
            draw(glyph, in: CGRect(x: bounds.midX - glyphSize / 2, y: top, width: glyphSize, height: glyphSize))
            label.draw(at: CGPoint(x: bounds.midX - label.size().width / 2, y: top + glyphSize + 2))
            return
        }
        let imageWidth: CGFloat = glyph == nil ? 0 : iconOnly ? 18 : 25
        let keyWidth = key.map { $0.size().width + 12 } ?? 0
        let decorations: CGFloat = (counter == nil ? 0 : 31) + (trailingSymbol == nil ? 0 : 16)
        var x = bounds.midX - (imageWidth + label.size().width + decorations + (key == nil ? 0 : keyWidth + 9)) / 2
        draw(glyph, in: CGRect(x: x, y: bounds.midY - 9, width: 18, height: 18))
        x += imageWidth
        label.draw(at: CGPoint(x: x, y: bounds.midY - label.size().height / 2))
        x += label.size().width
        if let counter {
            let text = NSAttributedString(string: String(counter), attributes: [
                .font: NSFont.monospacedDigitSystemFont(ofSize: 16, weight: .semibold), .foregroundColor: p.bar
            ])
            let width = max(22, text.size().width + 10)
            let pill = CGRect(x: x + 6, y: bounds.midY - 11, width: width, height: 22)
            p.ink.setFill()
            NSBezierPath(roundedRect: pill, xRadius: 11, yRadius: 11).fill()
            text.draw(at: CGPoint(x: pill.midX - text.size().width / 2, y: pill.midY - text.size().height / 2))
            x = pill.maxX + 3
        }
        if let trailingSymbol {
            draw(screenshotSymbol(trailingSymbol, color: ink, size: 11),
                 in: CGRect(x: x + 3, y: bounds.midY - 6, width: 12, height: 12))
        }
        if let key {
            x += 9
            let rect = CGRect(x: x, y: bounds.midY - 12, width: keyWidth, height: 24)
            ink.setStroke()
            NSBezierPath(roundedRect: rect, xRadius: 6, yRadius: 6).stroke()
            key.draw(at: CGPoint(x: rect.midX - key.size().width / 2, y: rect.midY - key.size().height / 2))
        }
    }

    private func draw(_ image: NSImage?, in rect: CGRect) {
        guard let image else { return }
        let scale = min(rect.width / image.size.width, rect.height / image.size.height)
        let size = CGSize(width: image.size.width * scale, height: image.size.height * scale)
        image.draw(in: CGRect(x: rect.midX - size.width / 2, y: rect.midY - size.height / 2,
                              width: size.width, height: size.height))
    }
}

@MainActor
func screenshotSymbol(_ name: String, color: NSColor, size: CGFloat = 16) -> NSImage? {
    NSImage(systemSymbolName: name, accessibilityDescription: nil)?
        .withSymbolConfiguration(.init(pointSize: size, weight: .medium))?
        .withSymbolConfiguration(.init(paletteColors: [color]))
}

@MainActor
func screenshotLabel(_ title: String = "", weight: NSFont.Weight = .regular) -> NSTextField {
    let label = NSTextField(labelWithString: title)
    label.font = .systemFont(ofSize: 16, weight: weight)
    label.lineBreakMode = .byTruncatingTail
    return label
}

@MainActor
final class ScreenshotEditorWindow: NSWindow {
    var intercept: ((NSEvent) -> Bool)?

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, attachedSheet == nil, intercept?(event) == true { return }
        super.sendEvent(event)
    }
}

@MainActor
final class ScreenshotChoice: NSView {
    let buttons: [ScreenshotButton]
    var changed: ((Int) -> Void)?
    var selection = 0 {
        didSet { for (index, button) in buttons.enumerated() { button.isSelected = index == selection } }
    }

    init(_ labels: [String], label: String) {
        buttons = labels.map { ScreenshotButton($0, style: .segment) }
        super.init(frame: .zero)
        setAccessibilityElement(false)
        setAccessibilityLabel(label)
        for (index, button) in buttons.enumerated() {
            addSubview(button)
            button.onPress = { [weak self] in self?.changed?(index) }
            button.onArrow = { [weak self] direction in
                guard let self else { return }
                let next = (index + direction + buttons.count) % buttons.count
                changed?(next)
                window?.makeFirstResponder(buttons[next])
            }
        }
        buttons.first?.isSelected = true
        wantsLayer = true
        layer?.cornerRadius = 9
        layer?.borderWidth = 1
    }

    required init?(coder: NSCoder) { nil }
    var naturalWidth: CGFloat { buttons.reduce(6) { $0 + $1.naturalWidth() } }
    override func viewDidChangeEffectiveAppearance() { super.viewDidChangeEffectiveAppearance(); needsLayout = true }

    override func layout() {
        super.layout()
        layer?.borderColor = ScreenshotEditorPalette(self).control.cgColor
        let width = (bounds.width - 6) / CGFloat(buttons.count)
        for (index, button) in buttons.enumerated() {
            button.frame = CGRect(x: 3 + CGFloat(index) * width, y: 3, width: width, height: bounds.height - 6)
        }
    }
}


@MainActor
final class ScreenshotRule: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        ScreenshotEditorPalette(self).line.setFill()
        bounds.fill()
    }
}
