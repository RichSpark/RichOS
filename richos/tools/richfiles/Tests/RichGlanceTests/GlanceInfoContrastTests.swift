import AppKit
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
            let overWhite = try render(.white, size: size, scale: scale)
            let overBlack = try render(.black, size: size, scale: scale)
            var strokes = 0
            var lowest = Double.infinity
            // Rows below the info button (26 pt at 12 pt from the top), where only the text is drawn.
            for y in (44 * scale)..<(overWhite.height - 1) {
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
            print("RichGlance overlay over white at \(scale)x: lowest \(lowest):1 beside \(strokes) stroke pixels")
            XCTAssertGreaterThan(strokes, 200, "the line was not drawn at \(scale)x")
            XCTAssertGreaterThanOrEqual(lowest, 4.5,
                                        "the lightest pixel beside a stroke over white gives \(lowest):1 at \(scale)x")
        }
    }

    private func render(_ color: NSColor, size: CGSize, scale: Int) throws -> Render {
        let suite = "glance-contrast-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let ground = Ground(frame: CGRect(origin: .zero, size: size))
        ground.color = color
        let info = GlanceInfoView(preferences: GlanceInfoPreferences(defaults: defaults))
        info.frame = ground.bounds
        ground.addSubview(info)
        info.reset(showIcon: true)
        info.finishLookup(nil, unavailable: false)
        info.toggleInfo()
        ground.layoutSubtreeIfNeeded()
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
        return Render(width: srgb.pixelsWide, height: srgb.pixelsHigh, luminance: luminance)
    }
}
