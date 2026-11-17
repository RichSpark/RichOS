import AppKit
@testable import RichFilesCore
@testable import RichGlance
import XCTest

/// The info overlay draws white text on the picture with no panel. Over a pure white picture the
/// first pixel beside every stroke must still give the text 4.5:1, as round 90's shadow does.
@MainActor
final class GlanceInfoContrastTests: XCTestCase {
    private final class Ground: NSView {
        var color = NSColor.white
        override var isFlipped: Bool { true }
        override func draw(_ dirtyRect: NSRect) {
            color.setFill()
            bounds.fill()
        }
    }

    private struct Render {
        let width: Int
        let height: Int
        let luminance: [Double]
        subscript(x: Int, y: Int) -> Double { luminance[y * width + x] }
    }

    func testNothingSavedLineHoldsAAOverWhite() throws {
        for scale in [1, 2] {
            let size = CGSize(width: 700, height: 300)
            let overWhite = try render(.white, size: size, scale: scale).image
            let overBlack = try render(.black, size: size, scale: scale).image
            // Rows below the info button (26 pt at 12 pt from the top), where only the text is drawn.
            let (lowest, strokes) = contrast(overWhite, overBlack, rows: (44 * scale)..<(overWhite.height - 1))
            print("RichGlance overlay over white at \(scale)x: lowest \(lowest):1 beside \(strokes) stroke pixels")
            XCTAssertGreaterThan(strokes, 200, "the line was not drawn at \(scale)x")
            XCTAssertGreaterThanOrEqual(lowest, 4.5,
                                        "the lightest pixel beside a stroke over white gives \(lowest):1 at \(scale)x")
        }
    }

    /// A long note scrolled until only the bottom 4 pt of its first line (the descenders) is left at
    /// the top of the text: those strokes keep their black edge like every other line.
    func testLineCutOffAtTheTopWhileScrollingHoldsAAOverWhite() throws {
        let note = GlanceMetadata(
            note: Array(repeating: "Foggy quay jumping pigs, gypsy query jpg.", count: 14).joined(separator: " "),
            labels: [], folder: "", updated: Date(timeIntervalSince1970: 0), embedded: false)
        for scale in [1, 2] {
            let size = CGSize(width: 700, height: 300)
            let white = try render(.white, size: size, scale: scale, metadata: note, cutLineTo: 4)
            let overBlack = try render(.black, size: size, scale: scale, metadata: note, cutLineTo: 4).image
            let top = Int(white.textTop.rounded()) * scale
            let (lowest, strokes) = contrast(white.image, overBlack, rows: top..<(top + 4 * scale))
            print("RichGlance cut-off line over white at \(scale)x: lowest \(lowest):1 beside \(strokes) stroke pixels")
            XCTAssertGreaterThan(strokes, 20 * scale, "the cut-off line was not drawn at \(scale)x")
            XCTAssertGreaterThanOrEqual(lowest, 4.5,
                                        "the lightest pixel beside a cut-off stroke over white gives \(lowest):1 at \(scale)x")
        }
    }

    /// The lowest ratio between a stroke pixel and the lightest pixel within 1 px that the text did
    /// not touch; strokes and untouched pixels are read from the same frame over black.
    private func contrast(_ overWhite: Render, _ overBlack: Render, rows: Range<Int>) -> (lowest: Double, strokes: Int) {
        var strokes = 0
        var lowest = Double.infinity
        for y in rows {
            for x in 1..<(overWhite.width - 1) where overBlack[x, y] >= 0.8 {
                strokes += 1
                let text = overWhite[x, y]
                var lightest = 0.0
                for dy in -1...1 {
                    for dx in -1...1 where overBlack[x + dx, y + dy] < 0.01 {
                        lightest = max(lightest, overWhite[x + dx, y + dy])
                    }
                }
                lowest = min(lowest, (max(text, lightest) + 0.05) / (min(text, lightest) + 0.05))
            }
        }
        return (lowest, strokes)
    }

    /// `cutLineTo` scrolls the text until that many points of its first line are left; `textTop` is
    /// where the text's visible area starts, in points from the top of the frame.
    private func render(_ color: NSColor, size: CGSize, scale: Int, metadata: GlanceMetadata? = nil,
                        cutLineTo: CGFloat? = nil) throws -> (image: Render, textTop: CGFloat) {
        let suite = "glance-contrast-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let ground = Ground(frame: CGRect(origin: .zero, size: size))
        ground.color = color
        let info = GlanceInfoView(preferences: GlanceInfoPreferences(defaults: defaults))
        info.frame = ground.bounds
        ground.addSubview(info)
        info.reset(showIcon: true)
        info.finishLookup(metadata, unavailable: false)
        info.toggleInfo()
        ground.layoutSubtreeIfNeeded()
        let scroll = try XCTUnwrap(info.subviews.compactMap { $0 as? NSScrollView }.first)
        if let cutLineTo {
            let text = try XCTUnwrap(scroll.documentView as? NSTextView)
            let layout = try XCTUnwrap(text.layoutManager)
            let line = layout.lineFragmentRect(forGlyphAt: 0, effectiveRange: nil)
            scroll.contentView.scroll(to: CGPoint(x: 0, y: line.maxY + text.textContainerOrigin.y - cutLineTo))
            scroll.reflectScrolledClipView(scroll.contentView)
            XCTAssertEqual(scroll.contentView.bounds.minY, line.maxY + text.textContainerOrigin.y - cutLineTo,
                           "the note is too short to scroll")
        }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: Int(size.width) * scale, pixelsHigh: Int(size.height) * scale,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        bitmap.size = size
        ground.cacheDisplay(in: ground.bounds, to: bitmap)
        let srgb = try XCTUnwrap(bitmap.converting(to: .sRGB, renderingIntent: .default))
        func linear(_ channel: CGFloat) -> Double {
            let value = Double(channel)
            return value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
        }
        var luminance = [Double](repeating: 0, count: srgb.pixelsWide * srgb.pixelsHigh)
        for y in 0..<srgb.pixelsHigh {
            for x in 0..<srgb.pixelsWide {
                let pixel = try XCTUnwrap(srgb.colorAt(x: x, y: y))
                luminance[y * srgb.pixelsWide + x] = 0.2126 * linear(pixel.redComponent)
                    + 0.7152 * linear(pixel.greenComponent) + 0.0722 * linear(pixel.blueComponent)
            }
        }
        return (Render(width: srgb.pixelsWide, height: srgb.pixelsHigh, luminance: luminance),
                scroll.convert(scroll.contentView.frame.origin, to: ground).y)
    }
}
