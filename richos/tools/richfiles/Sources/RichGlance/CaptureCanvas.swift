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
    var copy: (() -> Void)?
    var save: (() -> Void)?
    var scroll: ((ScrollingCapture.Mode) -> Void)?
    var escape: (() -> Void)?
    var cancel: (() -> Void)?
    private let actions = NSStackView()
    private let copyButton = NSButton(title: "Copy Image    ↵", target: nil, action: nil)
    private let saveButton = NSButton(title: "Save PNG…    ⌘S", target: nil, action: nil)
    private let scrollButton = NSButton(title: "Scrolling Capture…", target: nil, action: nil)
    private let retryButton = NSButton(title: "Start Over    Esc", target: nil, action: nil)
    private let cancelButton = NSButton(title: "Cancel Capture", target: nil, action: nil)
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
                                 (scrollButton, #selector(scrollPressed)), (retryButton, #selector(startOver)), (cancelButton, #selector(cancelPressed))] {
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
            case 53:
                if !event.isARepeat { escape?() }
                return
            case 36, 76: if !event.isARepeat { copy?() }; return
            default:
                guard let character = event.charactersIgnoringModifiers?.lowercased(), ["m", "r", "s"].contains(character) else {
                    super.keyDown(with: event)
                    return
                }
                guard !event.isARepeat else { return }
                if character == "r" { startOver(); return }
                if character == "s" { scrollPressed(); return }
                selection.toggleLens()
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
            let line = 1 / (window?.backingScaleFactor ?? 1)
            for start in stride(from: box.minY, to: box.maxY, by: 14) {
                for x in [box.minX, box.maxX] { vertical(x, start, min(start + 9, box.maxY), in: context, thickness: line) }
            }
            for start in stride(from: box.minX, to: box.maxX, by: 14) {
                for y in [box.minY, box.maxY] { horizontal(y, start, min(start + 9, box.maxX), in: context, thickness: line) }
            }
        }
        crosshair(at: target, context: context)
        if selection.phase != .aiming { drawCorners(at: target, context: context) }
        let position = lensPlacement(around: target)
        label(readout, in: position.label, size: 14, compact: true)
        if selection.showsLens { drawLens(in: position.lens, context: context) }
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
    @objc private func scrollPressed() {
        guard selection.phase == .refining, selection.canCapture else { return }
        let menu = NSMenu(title: "Scrolling Capture")
        for mode in ScrollingCapture.Mode.allCases {
            let item = menu.addItem(withTitle: mode.title, action: #selector(selectScrollMode(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = mode.rawValue
        }
        menu.popUp(positioning: nil, at: CGPoint(x: 0, y: scrollButton.bounds.height + 4), in: scrollButton)
    }

    @objc private func selectScrollMode(_ item: NSMenuItem) {
        guard let value = item.representedObject as? String,
              let mode = ScrollingCapture.Mode(rawValue: value) else { return }
        scroll?(mode)
    }

    @objc private func startOver() {
        guard selection.phase == .refining else { return }
        _ = resetSelection()
    }
    @objc private func cancelPressed() { cancel?() }

    func resetSelection() -> Bool {
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
        setAccessibilityValue(dimensions)
        window?.invalidateCursorRects(for: self)
        actions.isHidden = selection.phase != .refining
        copyButton.isHidden = !selection.canCapture
        saveButton.isHidden = !selection.canCapture
        scrollButton.isHidden = !selection.canCapture
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
        styleAction(scrollButton, primary: false)
        styleAction(retryButton, primary: false)
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
        let labelWidth: CGFloat = selection.showsLens ? outside : 136
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
