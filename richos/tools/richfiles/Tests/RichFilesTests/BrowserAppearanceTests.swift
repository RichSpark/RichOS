import SwiftUI
import XCTest
@testable import RichFiles

final class BrowserAppearanceTests: XCTestCase {
    func testSystemIsTheDefaultAndFollowsBothSystemAppearances() {
        XCTAssertEqual(BrowserTheme.allCases.map(\.title), ["Sovereign", "System", "Daybreak"])
        XCTAssertEqual(BrowserTheme.defaultTheme, .system)
        XCTAssertNil(BrowserTheme.system.override)
        XCTAssertEqual(BrowserTheme.system.resolve(system: .light), .light)
        XCTAssertEqual(BrowserTheme.system.resolve(system: .dark), .dark)
        XCTAssertEqual(BrowserTheme.sovereign.resolve(system: .light), .dark)
        XCTAssertEqual(BrowserTheme.daybreak.resolve(system: .dark), .light)
    }

    /// The search box's prompt and the Disk Usage footnotes are hints a user is meant to read, so
    /// they clear WCAG AA's 4.5:1 for normal text on every ground, in both themes (the system
    /// colors they used before measured 1.80:1, 2.19:1 and 3.51:1).
    func testHintTextClearsTheContrastFloorOnEveryGroundInBothThemes() {
        for scheme in [ColorScheme.light, .dark] {
            let hint = BrowserPalette.hintValue(scheme)
            let g = BrowserPalette.grounds(scheme)
            for (name, ground) in [("chrome", g.chrome), ("pane", g.pane), ("sidebar", g.sidebar), ("floating", g.floating)] {
                let ratio = Self.contrast(hint, ground)
                XCTAssertGreaterThanOrEqual(ratio, 4.5, "hint on \(name), \(scheme): \(ratio):1")
            }
        }
    }

    /// WCAG 2.x relative luminance and contrast ratio of two opaque sRGB colors.
    private static func contrast(_ a: UInt32, _ b: UInt32) -> Double {
        func luminance(_ value: UInt32) -> Double {
            let channel = { (shift: UInt32) -> Double in
                let c = Double((value >> shift) & 0xff) / 255
                return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
            }
            return 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
        }
        let (hi, lo) = (max(luminance(a), luminance(b)), min(luminance(a), luminance(b)))
        return (hi + 0.05) / (lo + 0.05)
    }
}
