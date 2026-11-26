import AppKit

/// The eye from RichGlance's app icon, for the places that stand for the app itself.
public enum GlanceEye {
    /// The menu bar item's size in points, the same 18 points as RichScribe's item.
    public static let menuBarPoints: CGFloat = 18

    /// The eye in black on clear, which macOS tints as a template for a light or dark menu bar.
    public static func templateImage(points: CGFloat = menuBarPoints) -> NSImage {
        let image = NSImage(size: NSSize(width: points, height: points), flipped: true) { rect in
            NSColor.black.setFill()
            path(fitting: rect).fill()
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "RichGlance"
        return image
    }

    /// The icon's eye (its 640-unit artwork: the eyelids from x 304 to 594 and y 46 to 208, and
    /// the eyeball of radius 39.1 about (449, 127)), scaled to `rect`'s width and centered.
    static func path(fitting rect: NSRect) -> NSBezierPath {
        let p = { (x: CGFloat, y: CGFloat) in NSPoint(x: x, y: y) }
        let eye = NSBezierPath()
        eye.windingRule = .evenOdd
        // The outer edge of the eyelids.
        eye.move(to: p(449, 208))
        eye.curve(to: p(314.9, 138.5), controlPoint1: p(403.8, 208), controlPoint2: p(358.7, 184.6))
        eye.line(to: p(304, 127))
        eye.line(to: p(314.9, 115.5))
        eye.curve(to: p(449, 46), controlPoint1: p(358.7, 69.4), controlPoint2: p(403.8, 46))
        eye.curve(to: p(583.1, 115.5), controlPoint1: p(494.2, 46), controlPoint2: p(539.3, 69.4))
        eye.line(to: p(594, 127))
        eye.line(to: p(583.1, 138.5))
        eye.curve(to: p(449, 208), controlPoint1: p(539.3, 184.6), controlPoint2: p(494.2, 208))
        eye.close()
        // The opening between the eyelids.
        eye.move(to: p(350.8, 127))
        eye.curve(to: p(449, 174.5), controlPoint1: p(384.1, 158.5), controlPoint2: p(417.1, 174.5))
        eye.curve(to: p(547.2, 127), controlPoint1: p(481, 174.5), controlPoint2: p(514, 158.5))
        eye.curve(to: p(449, 79.5), controlPoint1: p(514, 95.5), controlPoint2: p(481, 79.5))
        eye.curve(to: p(350.8, 127), controlPoint1: p(417, 79.5), controlPoint2: p(384, 95.5))
        eye.close()
        eye.append(NSBezierPath(ovalIn: NSRect(x: 449 - 39.1, y: 127 - 39.1, width: 78.2, height: 78.2)))
        let scale = rect.width / (594 - 304)
        var transform = AffineTransform(translationByX: rect.midX, byY: rect.midY)
        transform.scale(scale)
        transform.translate(x: -449, y: -127)
        eye.transform(using: transform)
        return eye
    }
}
