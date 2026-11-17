import AppKit
import RichFilesCore

@MainActor
final class CaptureCanvas: NSView {
    private static let lensSize: CGFloat = 168
    private static let rim: CGFloat = 4
    private static let cell: CGFloat = 12
    private static let clearance: CGFloat = 12
    private static let margin: CGFloat = 6
    private static let darkStroke = NSColor(srgbRed: 12 / 255, green: 19 / 255, blue: 34 / 255, alpha: 1)
    private static let lightStroke = NSColor(srgbRed: 253 / 255, green: 252 / 255, blue: 248 / 255, alpha: 1)
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
    var copy: (() -> Void)?
    var save: (() -> Void)?
    var cancel: (() -> Void)?
    private let actions = NSStackView()
    private let copyButton = NSButton(title: "Copy Image    ↵", target: nil, action: nil)
    private let saveButton = NSButton(title: "Save PNG…    ⌘S", target: nil, action: nil)
    private let cancelButton = NSButton(title: "Cancel    Esc", target: nil, action: nil)
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
        actions.orientation = .horizontal
        actions.spacing = 6
        actions.edgeInsets = NSEdgeInsets(top: 6, left: 6, bottom: 6, right: 6)
        actions.wantsLayer = true
        actions.layer?.cornerRadius = 12
        actions.layer?.borderWidth = 1
        for (button, action) in [(copyButton, #selector(copyPressed)), (saveButton, #selector(savePressed)),
                                 (cancelButton, #selector(cancelPressed))] {
            button.target = self
            button.action = action
            button.bezelStyle = .rounded
            button.font = .systemFont(ofSize: 16, weight: .medium)
            button.controlSize = .large
            button.heightAnchor.constraint(equalToConstant: 36).isActive = true
            actions.addArrangedSubview(button)
        }
        addSubview(actions)
        applyAppearance()
        refresh()
    }

    required init?(coder: NSCoder) { nil }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: selection.phase == .refining ? .arrow : Self.blankCursor)
    }

    override func mouseMoved(with event: NSEvent) { movePointer(event) }
    override func mouseDragged(with event: NSEvent) { movePointer(event) }
    override func mouseDown(with event: NSEvent) {
        if selection.phase == .placing {
            selection.movePointer(to: source(event))
            selection.shiftReturn()
        } else {
            selection.mouseDown(at: source(event))
        }
        refresh()
    }
    override func mouseUp(with event: NSEvent) {
        selection.mouseUp()
        refresh()
    }

    override func keyDown(with event: NSEvent) {
        let modifiers = event.modifierFlags.intersection([.shift, .command, .control, .option])
        if modifiers == [.shift], event.keyCode == 36 {
            if !event.isARepeat { selection.shiftReturn() }
        } else if modifiers == [.command], event.charactersIgnoringModifiers?.lowercased() == "s" {
            if !event.isARepeat { save?() }
            return
        } else if modifiers.isEmpty {
            switch event.keyCode {
            case 123: selection.nudge(dx: -1, dy: 0)
            case 124: selection.nudge(dx: 1, dy: 0)
            case 125: selection.nudge(dx: 0, dy: 1)
            case 126: selection.nudge(dx: 0, dy: -1)
            case 48: if !event.isARepeat { selection.switchCorner() }
            case 53: cancel?(); return
            case 36, 76: if !event.isARepeat { copy?() }; return
            default:
                guard event.charactersIgnoringModifiers?.lowercased() == "m" else {
                    super.keyDown(with: event)
                    return
                }
                if !event.isARepeat { selection.toggleLens() }
            }
        } else {
            super.keyDown(with: event)
            return
        }
        refresh()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyAppearance()
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        let target = geometry.viewPoint(selection.target)
        if let crop = selection.crop, !crop.isEmpty {
            let box = geometry.viewRect(crop)
            let shade = CGMutablePath()
            shade.addRect(bounds)
            shade.addRect(box)
            context.setFillColor(Self.darkStroke.withAlphaComponent(0.5).cgColor)
            context.addPath(shade)
            context.drawPath(using: .eoFill)
            for x in [box.minX, box.maxX] { vertical(x, box.minY, box.maxY, in: context) }
            for y in [box.minY, box.maxY] { horizontal(y, box.minX, box.maxX, in: context) }
        }
        crosshair(at: target, context: context)
        if selection.phase != .aiming { drawCorners(at: target, context: context) }
        let position = lensPlacement(around: target)
        if selection.showsLens { drawLens(in: position.lens, context: context) }
        label(dimensions, in: position.label, size: 16)
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
    @objc private func savePressed() { save?() }
    @objc private func cancelPressed() { cancel?() }

    private func source(_ event: NSEvent) -> CGPoint {
        geometry.sourcePoint(convert(event.locationInWindow, from: nil))
    }

    private func movePointer(_ event: NSEvent) {
        let before = selection.target
        selection.movePointer(to: source(event))
        if before != selection.target { refresh() }
    }

    private var dimensions: String {
        guard let crop = selection.crop else { return "Display \(image.width) × \(image.height) px" }
        return "Crop \(Int(crop.width)) × \(Int(crop.height)) px"
    }

    private func refresh() {
        failure = nil
        setAccessibilityValue(dimensions)
        window?.invalidateCursorRects(for: self)
        actions.isHidden = selection.phase != .refining
        copyButton.isHidden = !selection.canCapture
        saveButton.isHidden = !selection.canCapture
        if !actions.isHidden, let crop = selection.crop {
            let placement = lensPlacement(around: geometry.viewPoint(selection.target))
            actions.frame = actionPlacement(around: geometry.viewRect(crop), avoiding: placement)
        }
        needsDisplay = true
    }

    private func applyAppearance() {
        actions.layer?.backgroundColor = surface.cgColor
        actions.layer?.borderColor = border.cgColor
        styleAction(copyButton, primary: true)
        styleAction(saveButton, primary: false)
        styleAction(cancelButton, primary: false)
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
        let labelWidth: CGFloat = selection.phase == .aiming ? 222 : 188
        let labelHeight: CGFloat = 34
        let usable = bounds.insetBy(dx: Self.margin, dy: Self.margin)
        let other = geometry.viewPoint(selection.active == .first ? selection.second : selection.first)
        let side: CGFloat = selection.phase == .aiming || center.x <= other.x ? -1 : 1
        let direction: CGFloat = selection.phase == .aiming || center.y >= other.y ? 1 : -1
        let quadrants = [(side, direction), (-side, direction), (side, -direction), (-side, -direction)]
        for (x, y) in quadrants {
            let offset = outside / 2 + Self.clearance
            let position = CGPoint(x: center.x + x * offset, y: center.y + y * offset)
            let outer = CGRect(x: position.x - outside / 2, y: position.y - outside / 2, width: outside, height: outside)
            let pillX = min(max(usable.minX, position.x - labelWidth / 2), usable.maxX - labelWidth)
            let pillY = y > 0 ? outer.maxY + 8 : outer.minY - 8 - labelHeight
            let pill = selection.showsLens
                ? CGRect(x: pillX, y: pillY, width: labelWidth, height: labelHeight)
                : CGRect(x: x > 0 ? center.x + 16 : center.x - 16 - labelWidth,
                         y: y > 0 ? center.y + 16 : center.y - 16 - labelHeight, width: labelWidth, height: labelHeight)
            if usable.contains(pill) && (!selection.showsLens || usable.contains(outer)) {
                return (outer.insetBy(dx: Self.rim, dy: Self.rim), pill)
            }
        }
        let lens = CGRect(x: usable.midX - size / 2, y: usable.minY + Self.rim, width: size, height: size)
        return (lens, CGRect(x: usable.midX - labelWidth / 2, y: lens.maxY + 12, width: labelWidth, height: labelHeight))
    }

    private func actionPlacement(around box: CGRect, avoiding hud: (lens: CGRect, label: CGRect)) -> CGRect {
        let size = actions.fittingSize
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
        let gap: CGFloat? = selection.phase == .aiming ? 7 : nil
        for start in stride(from: 0 as CGFloat, to: bounds.height, by: 10) {
            let stop = min(start + 6, bounds.height)
            if let gap, stop > point.y - gap && start < point.y + gap { continue }
            vertical(point.x, start, stop, in: context)
        }
        for start in stride(from: 0 as CGFloat, to: bounds.width, by: 10) {
            let stop = min(start + 6, bounds.width)
            if let gap, stop > point.x - gap && start < point.x + gap { continue }
            horizontal(point.y, start, stop, in: context)
        }
    }

    private func vertical(_ x: CGFloat, _ top: CGFloat, _ bottom: CGFloat, in context: CGContext, thickness: CGFloat = 1) {
        context.setFillColor(Self.darkStroke.cgColor)
        context.fill(CGRect(x: x - thickness, y: top, width: thickness, height: bottom - top))
        context.setFillColor(Self.lightStroke.cgColor)
        context.fill(CGRect(x: x, y: top, width: thickness, height: bottom - top))
    }

    private func horizontal(_ y: CGFloat, _ left: CGFloat, _ right: CGFloat, in context: CGContext, thickness: CGFloat = 1) {
        context.setFillColor(Self.darkStroke.cgColor)
        context.fill(CGRect(x: left, y: y - thickness, width: right - left, height: thickness))
        context.setFillColor(Self.lightStroke.cgColor)
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
        context.setFillColor(Self.gold.cgColor)
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
        context.setStrokeColor(surface.cgColor)
        context.setLineWidth(Self.rim)
        context.strokeEllipse(in: rect.insetBy(dx: -Self.rim / 2, dy: -Self.rim / 2))
        context.setStrokeColor(border.cgColor)
        context.setLineWidth(1)
        context.strokeEllipse(in: rect)
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

    private func label(_ title: String, in rect: CGRect, size: CGFloat) {
        surface.setFill()
        let pill = NSBezierPath(roundedRect: rect, xRadius: 17, yRadius: 17)
        pill.fill()
        border.setStroke()
        pill.lineWidth = 1
        pill.stroke()
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        let text = title as NSString
        let font = NSFont.monospacedDigitSystemFont(ofSize: size, weight: .semibold)
        let attributes: [NSAttributedString.Key: Any] = [
            .foregroundColor: ink, .font: font, .paragraphStyle: paragraph
        ]
        let height = text.size(withAttributes: attributes).height
        text.draw(in: CGRect(x: rect.minX + 4, y: rect.midY - height / 2, width: rect.width - 8, height: height), withAttributes: attributes)
    }
}
