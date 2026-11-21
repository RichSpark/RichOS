import AppKit
import ImageIO
import UniformTypeIdentifiers

enum ScreenshotTool: String, CaseIterable, Codable {
    case select, arrow, text, box, highlight, steps, redact, crop

    var title: String { rawValue.capitalized }
    var key: String {
        switch self {
        case .select: "v"
        case .arrow: "a"
        case .text: "t"
        case .box: "b"
        case .highlight: "h"
        case .steps: "n"
        case .redact: "r"
        case .crop: "c"
        }
    }

    var symbol: String {
        switch self {
        case .select: "cursorarrow"
        case .arrow: "arrow.up.right"
        case .text: "text.bubble"
        case .box: "rectangle"
        case .highlight: "highlighter"
        case .steps: "1.circle"
        case .redact: "rectangle.fill"
        case .crop: "crop"
        }
    }
}

enum ScreenshotInk: String, CaseIterable, Codable {
    case red, yellow, blue, black

    var color: NSColor {
        switch self {
        case .red: NSColor(srgbRed: 0.70, green: 0.11, blue: 0.16, alpha: 1)
        case .yellow: NSColor(srgbRed: 1, green: 0.78, blue: 0.22, alpha: 1)
        case .blue: NSColor(srgbRed: 0.12, green: 0.33, blue: 0.68, alpha: 1)
        case .black: ScreenshotRenderer.ink
        }
    }

    var outline: NSColor { self == .yellow ? ScreenshotRenderer.ink : .white }
}

struct ScreenshotMark: Codable, Equatable, Identifiable {
    var id = UUID()
    var tool: ScreenshotTool
    var start: CGPoint
    var end: CGPoint
    var pointer: CGPoint?
    var text = ""
    var ink: ScreenshotInk = .red
    var large = false

    var rect: CGRect { ScreenshotRenderer.rect(from: start, to: end) }

    mutating func move(dx: CGFloat, dy: CGFloat) {
        start = start.offset(dx: dx, dy: dy)
        end = end.offset(dx: dx, dy: dy)
        pointer = pointer?.offset(dx: dx, dy: dy)
    }
}

struct ScreenshotEdits: Codable, Equatable {
    var crop: CGRect
    var marks: [ScreenshotMark] = []

    func number(for mark: ScreenshotMark) -> Int? {
        marks.filter { $0.tool == .steps }.firstIndex { $0.id == mark.id }.map { $0 + 1 }
    }

    mutating func reorderStep(_ id: UUID, by amount: Int) {
        let steps = marks.indices.filter { marks[$0].tool == .steps }
        guard let index = steps.firstIndex(where: { marks[$0].id == id }),
              steps.indices.contains(index + amount) else { return }
        marks.swapAt(steps[index], steps[index + amount])
    }
}

enum ScreenshotKind: String, Codable {
    case region, window, display, scrolling
    var title: String { self == .scrolling ? "Scrolling capture" : rawValue.capitalized }
    var symbol: String {
        switch self {
        case .region: "viewfinder"
        case .window: "macwindow"
        case .display: "display"
        case .scrolling: "arrow.up.and.down.square"
        }
    }
}

struct ScreenshotDocument {
    let id: UUID
    let created: Date
    let original: CGImage
    let scale: CGFloat
    let kind: ScreenshotKind
    let name: String?
    var edits: ScreenshotEdits

    init(original: CGImage, scale: CGFloat, kind: ScreenshotKind = .region,
         id: UUID = UUID(), created: Date = Date(), name: String? = nil, edits: ScreenshotEdits? = nil) {
        self.id = id
        self.created = created
        self.original = original
        self.scale = max(1, scale)
        self.kind = kind
        self.name = name
        self.edits = edits ?? ScreenshotEdits(crop: CGRect(x: 0, y: 0, width: original.width, height: original.height))
    }

    var bounds: CGRect { CGRect(x: 0, y: 0, width: original.width, height: original.height) }
    var crop: CGRect { edits.crop.integral.intersection(bounds) }
    var title: String { name ?? (kind == .scrolling ? "Scrolling screenshot" : "\(kind.title) screenshot") }
    var hasRedactions: Bool { edits.marks.contains { $0.tool == .redact } }
}

enum ScreenshotRenderer {
    static let ink = NSColor(srgbRed: 12.0 / 255, green: 19.0 / 255, blue: 34.0 / 255, alpha: 1)

    static func rect(from a: CGPoint, to b: CGPoint) -> CGRect {
        CGRect(x: min(a.x, b.x), y: min(a.y, b.y), width: abs(a.x - b.x), height: abs(a.y - b.y))
    }

    static func flattened(_ document: ScreenshotDocument) -> CGImage? {
        render(document, crop: document.crop, scale: 1)
    }

    static func thumbnail(_ document: ScreenshotDocument) -> CGImage? {
        let crop = document.crop
        guard !crop.isEmpty else { return nil }
        let width = thumbnailWidth(for: crop)
        let scale = max(width / crop.width, 78 / crop.height)
        let sampleWidth = min(crop.width, width / scale)
        let sampleHeight = min(crop.height, 78 / scale)
        let sample = CGRect(x: crop.midX - sampleWidth / 2, y: crop.minY,
                            width: sampleWidth, height: sampleHeight).integral.intersection(crop)
        return render(document, crop: sample, scale: 2 * scale)
    }

    static func thumbnailWidth(for crop: CGRect) -> CGFloat {
        guard crop.height > 0 else { return 56 }
        return min(150, max(56, (78 * crop.width / crop.height).rounded()))
    }

    private static func render(_ document: ScreenshotDocument, crop: CGRect, scale: CGFloat) -> CGImage? {
        guard !crop.isNull, !crop.isEmpty,
              let source = document.original.cropping(to: crop),
              let space = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        let size = CGSize(width: max(1, (crop.width * scale).rounded()), height: max(1, (crop.height * scale).rounded()))
        guard let context = CGContext(data: nil, width: Int(size.width), height: Int(size.height),
                                      bitsPerComponent: 8, bytesPerRow: 0, space: space,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        context.interpolationQuality = scale < 1 ? .medium : .none
        context.setBlendMode(.copy)
        context.draw(source, in: CGRect(origin: .zero, size: size))
        context.setBlendMode(.normal)
        let sx = size.width / crop.width
        let sy = size.height / crop.height
        context.translateBy(x: -crop.minX * sx, y: crop.maxY * sy)
        context.scaleBy(x: sx, y: -sy)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: true)
        drawMarks(document.edits, scale: document.scale, in: context)
        NSGraphicsContext.restoreGraphicsState()
        return context.makeImage()
    }

    static func png(_ image: CGImage) -> Data? {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? data as Data : nil
    }

    static func copy(_ png: Data, to clipboard: NSPasteboard = .general) -> Bool {
        let item = NSPasteboardItem()
        guard item.setData(png, forType: .png) else { return false }
        clipboard.clearContents()
        return clipboard.writeObjects([item])
    }

    static func drawMarks(_ edits: ScreenshotEdits, scale: CGFloat, in context: CGContext) {
        var number = 0
        for mark in edits.marks where mark.tool != .redact {
            if mark.tool == .steps { number += 1 }
            draw(mark, number: number, scale: scale, in: context)
        }
        // Opaque redactions are composited last, even when a highlight or caption overlaps them.
        for mark in edits.marks where mark.tool == .redact {
            draw(mark, number: 0, scale: scale, in: context)
        }
    }

    static func draw(_ mark: ScreenshotMark, number: Int, scale: CGFloat, in context: CGContext) {
        let unit = scale * (mark.large ? 1.5 : 1)
        let color = mark.ink.color
        let outline = mark.ink.outline
        let rect = mark.rect
        context.saveGState()
        defer { context.restoreGState() }
        context.setLineCap(.round)
        context.setLineJoin(.round)
        switch mark.tool {
        case .arrow:
            let angle = atan2(mark.end.y - mark.start.y, mark.end.x - mark.start.x)
            let length = min(15 * unit, hypot(mark.end.y - mark.start.y, mark.end.x - mark.start.x) * 0.65)
            let path = CGMutablePath()
            path.move(to: mark.start)
            path.addLine(to: mark.end)
            for wing in [-CGFloat.pi / 5, .pi / 5] {
                path.move(to: mark.end)
                path.addLine(to: mark.end.offset(dx: -length * cos(angle + wing), dy: -length * sin(angle + wing)))
            }
            context.saveGState()
            // Shadow offsets are expressed in captured pixels and follow the rendered zoom.
            let transform = context.ctm
            let dx = hypot(transform.a, transform.b)
            let dy = hypot(transform.c, transform.d)
            let shadowScale: CGFloat = mark.large ? 1.5 : 1
            context.setShadow(offset: CGSize(width: 2 * dx * shadowScale, height: -2 * dy * shadowScale),
                              blur: 2.5 * dx * shadowScale, color: ink.withAlphaComponent(0.45).cgColor)
            context.setStrokeColor(outline.cgColor)
            context.setLineWidth(5 * unit + 2)
            context.addPath(path)
            context.strokePath()
            context.restoreGState()
            stroke(path, color: color, outline: outline, width: 5 * unit, in: context)
        case .box:
            stroke(CGPath(rect: rect, transform: nil), color: color, outline: outline, width: 4 * unit, in: context)
        case .highlight:
            context.setFillColor(color.withAlphaComponent(0.28).cgColor)
            context.fill(rect)
            stroke(CGPath(rect: rect, transform: nil), color: color, outline: outline, width: unit, in: context)
        case .steps:
            let size = 30 * unit
            let bubble = CGRect(x: mark.start.x - size / 2, y: mark.start.y - size / 2, width: size, height: size)
            let path = CGPath(ellipseIn: bubble, transform: nil)
            context.addPath(path)
            context.setFillColor(color.cgColor)
            context.fillPath()
            context.addPath(path)
            context.setLineWidth(2 * scale)
            context.setStrokeColor(outline.cgColor)
            context.strokePath()
            let label = NSAttributedString(string: String(number), attributes: [
                .font: NSFont.monospacedDigitSystemFont(ofSize: (number >= 100 ? 12 : 18) * unit, weight: .bold),
                .foregroundColor: outline,
            ])
            let labelSize = label.size()
            label.draw(at: CGPoint(x: bubble.midX - labelSize.width / 2, y: bubble.midY - labelSize.height / 2))
        case .text:
            if let pointer = mark.pointer, !rect.contains(pointer) {
                let dx = pointer.x - rect.midX
                let dy = pointer.y - rect.midY
                let ratio = max(abs(dx) / max(1, rect.width / 2), abs(dy) / max(1, rect.height / 2))
                let base = CGPoint(x: rect.midX + 0.9 * dx / ratio, y: rect.midY + 0.9 * dy / ratio)
                let length = hypot(dx, dy)
                let width = min(9 * unit, 0.3 * min(rect.width, rect.height))
                let side = CGPoint(x: dy / length * width, y: -dx / length * width)
                let tail = CGMutablePath()
                tail.move(to: base.offset(dx: side.x, dy: side.y))
                tail.addLine(to: pointer)
                tail.addLine(to: base.offset(dx: -side.x, dy: -side.y))
                tail.closeSubpath()
                context.setFillColor(color.cgColor)
                context.setStrokeColor(outline.cgColor)
                context.setLineWidth(2 * scale)
                context.addPath(tail)
                context.drawPath(using: .fillStroke)
            }
            let path = CGPath(roundedRect: rect, cornerWidth: 6 * unit, cornerHeight: 6 * unit, transform: nil)
            context.setFillColor(color.cgColor)
            context.addPath(path)
            context.fillPath()
            context.setStrokeColor(outline.cgColor)
            context.setLineWidth(2 * scale)
            context.addPath(path)
            context.strokePath()
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineBreakMode = .byWordWrapping
            NSAttributedString(string: mark.text, attributes: [
                .font: NSFont.systemFont(ofSize: 16 * unit, weight: .semibold),
                .foregroundColor: outline,
                .paragraphStyle: paragraph,
            ]).draw(with: rect.insetBy(dx: 8 * scale, dy: 5 * scale), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        case .redact:
            // Integral, fully opaque coverage. Draw the outline inside the covered rectangle.
            let covered = rect.integral
            context.setShouldAntialias(false)
            context.setFillColor(ink.cgColor)
            context.fill(covered)
            if covered.width > 4 * scale, covered.height > 4 * scale {
                context.setLineWidth(scale)
                context.setStrokeColor(NSColor.white.cgColor)
                context.stroke(covered.insetBy(dx: 1.5 * scale, dy: 1.5 * scale))
            }
        case .select, .crop:
            break
        }
    }

    private static func stroke(_ path: CGPath, color: NSColor, outline: NSColor, width: CGFloat, in context: CGContext) {
        context.setStrokeColor(outline.cgColor)
        context.setLineWidth(width + 2)
        context.addPath(path)
        context.strokePath()
        context.setStrokeColor(color.cgColor)
        context.setLineWidth(width)
        context.addPath(path)
        context.strokePath()
    }
}

extension CGPoint {
    fileprivate func offset(dx: CGFloat, dy: CGFloat) -> CGPoint { CGPoint(x: x + dx, y: y + dy) }
}
