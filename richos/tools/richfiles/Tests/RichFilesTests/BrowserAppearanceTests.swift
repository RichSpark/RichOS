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

    /// Round 107.1's item states, scroll bars, sidebar tree and quick-info box, in both themes:
    /// readable text at 4.5:1 and indicators at 3:1 against what they sit on. A token with
    /// opacity is composited onto its ground first.
    ///
    /// Declared exemptions, as in the round's README: the hover tint and the selected fill carry
    /// no state on their own (the name on them is checked, and the selected state is the 1 px
    /// line, checked against the pane, the plain fill and the deeper fill under the pointer).
    func testRound107ItemStatesTreeAndQuickInfoClearTheContrastFloorInBothThemes() {
        for scheme in [ColorScheme.light, .dark] {
            let g = BrowserPalette.grounds(scheme)
            let ink = BrowserPalette.inkValue(scheme)
            let muted = { (ground: UInt32) in Self.composite(ink, BrowserPalette.mutedOpacity(scheme), on: ground) }
            let hover = { (ground: UInt32) in Self.composite(ink, BrowserPalette.hoverOpacity(scheme), on: ground) }
            let selection = BrowserPalette.selectionValues(scheme)
            let scroll = BrowserPalette.scrollValues(scheme)
            let track = Self.composite(scroll.track, scroll.trackOpacity, on: g.pane)
            let current = BrowserPalette.currentFolderValue(scheme)
            let button = BrowserPalette.sheetButtonValues(scheme)
            let pairs: [(String, UInt32, UInt32, Double)] = [
                ("name on the selected fill", ink, selection.fill, 4.5),
                ("name on the selected fill under the pointer", ink, selection.fillHover, 4.5),
                ("name on the hover tint", ink, hover(g.pane), 4.5),
                ("selected line against the pane", selection.edge, g.pane, 3),
                ("selected line against its fill", selection.edge, selection.fill, 3),
                ("keyboard-focus outline against the pane", selection.edge, g.pane, 3),
                ("inactive pane's line against the pane", muted(g.pane), g.pane, 3),
                ("scroll thumb against its track", scroll.thumb, track, 3),
                ("scroll thumb, pointed at, against its track", scroll.thumbHover, track, 3),
                ("sidebar scroll thumb on the sidebar", scroll.thumb, g.sidebar, 3),
                ("sidebar scroll thumb, pointed at", scroll.thumbHover, g.sidebar, 3),
                ("quick-info label", muted(g.floating), g.floating, 4.5),
                ("quick-info value", ink, g.floating, 4.5),
                ("quick-info border", BrowserPalette.controlLineValue(scheme), g.pane, 3),
                ("Open With's Default tag on the chosen app", muted(selection.fill), selection.fill, 4.5),
                ("folder name in the tree", ink, g.sidebar, 4.5),
                ("folder name on the row hover", ink, hover(g.sidebar), 4.5),
                ("current folder's name on its fill", ink, current, 4.5),
                ("arrow on the sidebar", muted(g.sidebar), g.sidebar, 3),
                ("arrow on a hovered row", muted(hover(g.sidebar)), hover(g.sidebar), 3),
                ("arrow on the current folder", muted(current), current, 3),
                ("arrow under the pointer", ink, hover(g.sidebar), 3),
                ("folder icon on the sidebar", BrowserPalette.folderValue(scheme), g.sidebar, 3),
                ("folder icon on the current folder", BrowserPalette.folderValue(scheme), current, 3),
                ("current folder's bar on its fill", BrowserPalette.signalValue(scheme), current, 3),
                ("selected line against its fill under the pointer", selection.edge, selection.fillHover, 3),
                ("Open With chooser's Open label on its fill", button.primaryLabel, button.primaryFill, 4.5),
                ("Open With chooser's Open button against the sheet", button.primaryFill, g.floating, 3),
                ("Open With chooser's Cancel label on its fill", button.label, button.fill, 4.5),
                ("Open With chooser's Cancel outline against the sheet", BrowserPalette.controlLineValue(scheme), g.floating, 3)
            ]
            for (name, foreground, ground, floor) in pairs {
                let ratio = Self.contrast(foreground, ground)
                XCTAssertGreaterThanOrEqual(ratio, floor, "\(name), \(scheme): \(ratio):1")
            }
        }
    }

    /// One sRGB color at `opacity` over an opaque ground, channel by channel.
    private static func composite(_ color: UInt32, _ opacity: Double, on ground: UInt32) -> UInt32 {
        [16, 8, 0].reduce(UInt32(0)) { value, shift in
            let top = Double((color >> UInt32(shift)) & 0xff)
            let bottom = Double((ground >> UInt32(shift)) & 0xff)
            return value | (UInt32((opacity * top + (1 - opacity) * bottom).rounded()) << UInt32(shift))
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
