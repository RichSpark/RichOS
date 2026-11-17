import AppKit
import RichFilesCore

@MainActor
final class Canvas: NSView {
    var step: ((Int) -> Void)?
    var close: (() -> Void)?

    private var viewport = GlanceViewport()
    private var image: NSImage?
    private var message: String?
    private var anchor: (pointer: CGPoint, image: CGPoint)?
    private var dragPoint: CGPoint?
    private var feedbackTimer: Timer?
    private let feedback = NSTextField(labelWithString: "")

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    init() {
        super.init(frame: .zero)
        setAccessibilityElement(true)
        setAccessibilityRole(.image)
        feedback.font = .systemFont(ofSize: 16, weight: .medium)
        feedback.isHidden = true
        feedback.translatesAutoresizingMaskIntoConstraints = false
        feedback.setAccessibilityElement(false)
        addSubview(feedback)
        NSLayoutConstraint.activate([
            feedback.centerXAnchor.constraint(equalTo: centerXAnchor),
            feedback.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -24)
        ])
    }

    required init?(coder: NSCoder) { nil }

    func show(_ decoded: GlanceImage?, filename: String, message: String? = nil) {
        image = decoded.map { NSImage(cgImage: $0.bitmap, size: $0.size) }
        self.message = message
        setAccessibilityLabel(filename)
        setAccessibilityValue(message)
        viewport.reset(imageSize: decoded?.size ?? .zero, in: bounds.size)
        anchor = nil
        dragPoint = nil
        refresh()
    }

    func announce(_ text: String?) {
        feedbackTimer?.invalidate()
        feedback.isHidden = text == nil
        if let text {
            feedback.stringValue = text
            // The image's accessible value follows the same feedback as the visual indicator.
            setAccessibilityValue(text)
            NSAccessibility.post(element: self, notification: .valueChanged)
            feedbackTimer = Timer.scheduledTimer(withTimeInterval: 1.3, repeats: false) { [weak self] _ in
                MainActor.assumeIsolated { self?.announce(nil) }
            }
        } else {
            setAccessibilityValue(message)
        }
        needsDisplay = true
    }

    override func layout() {
        super.layout()
        if viewport.size != bounds.size {
            viewport.resize(to: bounds.size)
            anchor = nil
            refresh()
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updatePalette()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        updatePalette()
    }

    private func updatePalette() {
        let colors = palette
        feedback.textColor = colors.ink
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        let colors = palette
        colors.ground.setFill()
        dirtyRect.fill()
        NSGraphicsContext.current?.imageInterpolation = .high
        image?.draw(in: viewport.imageRect, from: .zero, operation: .sourceOver,
                    fraction: 1, respectFlipped: true, hints: nil)
        if let message {
            let label = NSAttributedString(string: message, attributes: [
                .font: NSFont.systemFont(ofSize: 16), .foregroundColor: colors.ink
            ])
            let rect = bounds.insetBy(dx: 24, dy: 12)
            let size = label.boundingRect(with: rect.size, options: [.usesLineFragmentOrigin]).size
            label.draw(in: CGRect(x: bounds.midX - ceil(size.width) / 2,
                                  y: bounds.midY - ceil(size.height) / 2,
                                  width: ceil(size.width) + 1, height: ceil(size.height) + 1))
        }
        if !feedback.isHidden {
            let frame = feedback.frame.insetBy(dx: -16, dy: -6).insetBy(dx: 0.5, dy: 0.5)
            let pill = NSBezierPath(roundedRect: frame, xRadius: frame.height / 2, yRadius: frame.height / 2)
            colors.floating.setFill()
            pill.fill()
            colors.border.setStroke()
            pill.stroke()
        }
    }

    override func keyDown(with event: NSEvent) {
        let modifiers = event.modifierFlags.intersection([.command, .option, .control, .shift])
        guard modifiers.isEmpty else { return super.keyDown(with: event) }
        switch event.keyCode {
        case 53: close?()
        case 123: step?(-1)
        case 124: step?(1)
        default: super.keyDown(with: event)
        }
    }

    override func scrollWheel(with event: NSEvent) {
        guard image != nil, event.modifierFlags.contains(.control) else {
            anchor = nil
            return super.scrollWheel(with: event)
        }
        guard event.momentumPhase.isEmpty, event.scrollingDeltaY != 0 else { return }
        let pointer = convert(event.locationInWindow, from: nil)
        if event.phase.contains(.began) { anchor = nil }
        let fixed: CGPoint
        if let anchor, hypot(anchor.pointer.x - pointer.x, anchor.pointer.y - pointer.y) <= 0.5 {
            fixed = anchor.image
        } else {
            fixed = viewport.imagePoint(at: pointer)
            anchor = (pointer, fixed)
        }
        let delta = event.scrollingDeltaY * (event.hasPreciseScrollingDeltas ? 1 : 16)
        viewport.zoom(by: exp(max(-60, min(60, delta)) * 0.012), at: pointer, holding: fixed)
        if viewport.isFit { anchor = nil }
        announce(viewport.isFit ? "Fit to window" : "\(Int((viewport.scale * 100).rounded()))%")
        refresh()
    }

    override func resetCursorRects() {
        super.resetCursorRects()
        if viewport.canPan { addCursorRect(bounds, cursor: .openHand) }
    }

    override func mouseDown(with event: NSEvent) {
        guard viewport.canPan else { return }
        anchor = nil
        dragPoint = convert(event.locationInWindow, from: nil)
        NSCursor.closedHand.set()
    }

    override func mouseDragged(with event: NSEvent) {
        guard let last = dragPoint else { return }
        let point = convert(event.locationInWindow, from: nil)
        viewport.pan(by: CGPoint(x: point.x - last.x, y: point.y - last.y))
        dragPoint = point
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard dragPoint != nil else { return }
        dragPoint = nil
        (viewport.canPan ? NSCursor.openHand : .arrow).set()
    }

    private func refresh() {
        window?.invalidateCursorRects(for: self)
        needsDisplay = true
    }

    private var palette: (ground: NSColor, ink: NSColor, floating: NSColor, border: NSColor) {
        func rgb(_ value: UInt32) -> NSColor {
            NSColor(srgbRed: CGFloat(value >> 16) / 255, green: CGFloat((value >> 8) & 255) / 255,
                    blue: CGFloat(value & 255) / 255, alpha: 1)
        }
        if effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua {
            return (rgb(0x060a14), rgb(0xdfe4ee), rgb(0x182440), rgb(0x9399a5))
        }
        return (rgb(0xdcd7cb), rgb(0x0c1322), rgb(0xfdfcf8), rgb(0x53575e))
    }
}
