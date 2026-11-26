import AppKit
import CoreText
import SwiftUI

enum BrowserTheme: String, CaseIterable, Identifiable {
    case sovereign, system, daybreak

    static let preferenceKey = "richfiles.theme"
    static let defaultTheme = Self.system

    var id: Self { self }

    var title: String {
        switch self {
        case .sovereign: "Sovereign"
        case .system: "System"
        case .daybreak: "Daybreak"
        }
    }

    var override: ColorScheme? {
        switch self {
        case .sovereign: .dark
        case .system: nil
        case .daybreak: .light
        }
    }

    func resolve(system: ColorScheme) -> ColorScheme {
        override ?? system
    }
}

struct BrowserAppearance: ViewModifier {
    @AppStorage(BrowserTheme.preferenceKey) private var theme = BrowserTheme.defaultTheme
    @Environment(\.colorScheme) private var systemScheme
    var setsWindowAppearance = false

    func body(content: Content) -> some View {
        let scheme = theme.resolve(system: systemScheme)
        let palette = BrowserPalette(scheme)
        content
            .environment(\.colorScheme, scheme)
            .tint(palette.signal)
            .foregroundStyle(palette.ink)
            .preferredColorScheme(setsWindowAppearance ? theme.override : nil)
    }
}

struct BrowserPalette {
    let chrome: Color
    let pane: Color
    let sidebar: Color
    let floating: Color
    let ink: Color
    let muted: Color
    let line: Color
    let controlLine: Color
    let signal: Color
    let selected: Color
    let inactiveSelection: Color
    let folder: Color
    let danger: Color
    let attention: Color
    let hover: Color
    let hint: Color
    /// The quiet item states of round 107.1 (catalog entries 4, 6, 7): one light fill and one
    /// thin line for a selected item, a slightly deeper fill while the pointer is on it.
    let selectionFill: Color
    let selectionFillHover: Color
    let selectionEdge: Color
    /// The file pane's and sidebar's scroll bars (entries 9 and 15).
    let scrollThumb: Color
    let scrollThumbHover: Color
    let scrollTrack: Color

    /// The four opaque grounds text sits on: the bars, the file pane, the sidebar and a sheet.
    static func grounds(_ scheme: ColorScheme) -> (chrome: UInt32, pane: UInt32, sidebar: UInt32, floating: UInt32) {
        scheme == .dark ? (0x0c1322, 0x0a1120, 0x0a101b, 0x182440) : (0xeae6dd, 0xfdfcf8, 0xefebe0, 0xfdfcf8)
    }

    /// HINT TEXT A USER IS MEANT TO READ: the search box's prompt and a sheet's footnote. It is
    /// opaque, so its contrast is one fixed number on each ground, computed in
    /// `BrowserAppearanceTests` (lowest 5.83:1 light on the chrome, 6.29:1 dark on a sheet). The
    /// system placeholder and secondary-label colors it replaces measured 1.80:1 light and
    /// 2.19:1 dark (the search prompt) and 3.51:1 light (the Disk Usage footnote) in a
    /// walk of the packaged app, below the 4.5:1 floor for text meant to be read.
    static func hintValue(_ scheme: ColorScheme) -> UInt32 {
        scheme == .dark ? 0xa0a6b1 : 0x53575e
    }

    /// The selected item's fill, its fill under the pointer and its 1 px line. The line carries
    /// the state (3:1 for an indicator); the fill only tells a selected item from the
    /// keyboard-focus outline. Computed in `BrowserAppearanceTests`. The dark line is a step
    /// lighter than round 107.1's #7d6f48, which measured 2.86:1 against the fill under the
    /// pointer; #84754c measures 3.12:1 there, 3.46:1 on the plain fill and 4.16:1 on the pane.
    static func selectionValues(_ scheme: ColorScheme) -> (fill: UInt32, fillHover: UInt32, edge: UInt32) {
        scheme == .dark ? (0x1b2335, 0x222b3e, 0x84754c) : (0xf3ecda, 0xede4cd, 0x8f7a48)
    }

    /// The scroll bar's thumb, its thumb when pointed at or dragged, and its track (with opacity).
    static func scrollValues(_ scheme: ColorScheme) -> (thumb: UInt32, thumbHover: UInt32, track: UInt32, trackOpacity: Double) {
        scheme == .dark ? (0x5d6b8c, 0x6b7ea8, 0x4c6087, 0.16) : (0x767c8d, 0x5d6372, 0x3c465f, 0.10)
    }

    /// A sheet's buttons as round 107.1 draws them (`.pbtn` and `.pbtn.def`): the default one is
    /// the signal color with the pane color for its bold label (6.65:1 light, 7.80:1 dark), the
    /// other one ink on the chrome color inside the control line. macOS's own blue default
    /// button left at most 3.75:1 for its label in the light theme.
    static func sheetButtonValues(_ scheme: ColorScheme) -> (primaryFill: UInt32, primaryLabel: UInt32, fill: UInt32, label: UInt32) {
        let g = grounds(scheme)
        return (signalValue(scheme), g.pane, g.chrome, inkValue(scheme))
    }

    /// The ink color as an opaque value; `muted` and `hover` are this ink at an opacity.
    static func inkValue(_ scheme: ColorScheme) -> UInt32 { scheme == .dark ? 0xdfe4ee : 0x0c1322 }
    static func mutedOpacity(_ scheme: ColorScheme) -> Double { scheme == .dark ? 0.64 : 0.68 }
    static func hoverOpacity(_ scheme: ColorScheme) -> Double { scheme == .dark ? 0.07 : 0.06 }
    static func controlLineValue(_ scheme: ColorScheme) -> UInt32 { scheme == .dark ? 0x6b7ea8 : 0x767c8d }
    static func folderValue(_ scheme: ColorScheme) -> UInt32 { scheme == .dark ? 0x7e92b8 : 0x4c6087 }
    static func currentFolderValue(_ scheme: ColorScheme) -> UInt32 { scheme == .dark ? 0x3a3d46 : 0xe4dbc5 }
    static func signalValue(_ scheme: ColorScheme) -> UInt32 { scheme == .dark ? 0xc2a35c : 0x715715 }

    init(_ scheme: ColorScheme) {
        let dark = scheme == .dark
        let grounds = Self.grounds(scheme)
        chrome = Self.rgb(grounds.chrome)
        pane = Self.rgb(grounds.pane)
        sidebar = Self.rgb(grounds.sidebar)
        floating = Self.rgb(grounds.floating)
        ink = Self.rgb(dark ? 0xdfe4ee : 0x0c1322)
        muted = ink.opacity(dark ? 0.64 : 0.68)
        hint = Self.rgb(Self.hintValue(scheme))
        line = Self.rgb(dark ? 0x4c6087 : 0x3c465f).opacity(dark ? 0.26 : 0.24)
        controlLine = Self.rgb(dark ? 0x6b7ea8 : 0x767c8d)
        signal = Self.rgb(dark ? 0xc2a35c : 0x715715)
        selected = Self.rgb(dark ? 0x3a3d46 : 0xe4dbc5)
        inactiveSelection = Self.rgb(dark ? 0x1f2a44 : 0xe6e2d8)
        folder = Self.rgb(dark ? 0x7e92b8 : 0x4c6087)
        danger = Self.rgb(dark ? 0xe8837c : 0x8a2f28)
        attention = Self.rgb(dark ? 0xe09a55 : 0x8c4a1b)
        hover = ink.opacity(dark ? 0.07 : 0.06)
        let selection = Self.selectionValues(scheme)
        selectionFill = Self.rgb(selection.fill)
        selectionFillHover = Self.rgb(selection.fillHover)
        selectionEdge = Self.rgb(selection.edge)
        let scroll = Self.scrollValues(scheme)
        scrollThumb = Self.rgb(scroll.thumb)
        scrollThumbHover = Self.rgb(scroll.thumbHover)
        scrollTrack = Self.rgb(scroll.track).opacity(scroll.trackOpacity)
    }

    static func rgb(_ value: UInt32) -> Color {
        Color(red: Double((value >> 16) & 0xff) / 255,
              green: Double((value >> 8) & 0xff) / 255,
              blue: Double(value & 0xff) / 255)
    }
}

enum BrowserTypography {
    private static let inter = register("Inter-Variable", face: "InterVariable")
    private static let newsreader = register("Newsreader-Regular", face: "Newsreader16pt-Regular")

    static func reading(_ size: CGFloat) -> Font {
        inter ? .custom("InterVariable", size: size) : .system(size: size)
    }

    /// The AppKit font that draws `reading(size)`, for measuring names and for AppKit text fields.
    static func readingNSFont(_ size: CGFloat) -> NSFont {
        (inter ? NSFont(name: "InterVariable", size: size) : nil) ?? .systemFont(ofSize: size)
    }

    static func heading(_ size: CGFloat) -> Font {
        newsreader ? .custom("Newsreader16pt-Regular", size: size) : .system(size: size, design: .serif)
    }

    private static func register(_ file: String, face: String) -> Bool {
        let bundles = [Bundle.main, Bundle(url: RichFilesInstallation.bundledApplication)].compactMap { $0 }
        guard let url = bundles.lazy.compactMap({ $0.url(forResource: file, withExtension: "ttf") }).first else {
            return false
        }
        CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
        let font = CTFontCreateWithName(face as CFString, 16, nil)
        return CTFontCopyPostScriptName(font) as String == face
    }
}

/// A sheet's Cancel and Open, in the app's colors (round 107.1's chooser). A disabled button is
/// drawn at half strength; WCAG exempts an inactive control, and it carries no readable state.
struct SheetButtonStyle: ButtonStyle {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.isEnabled) private var enabled
    var primary = false

    func makeBody(configuration: Configuration) -> some View {
        let values = BrowserPalette.sheetButtonValues(scheme)
        let line = BrowserPalette.rgb(primary ? values.primaryFill : BrowserPalette.controlLineValue(scheme))
        let shape = RoundedRectangle(cornerRadius: 7)
        configuration.label
            .fontWeight(primary ? .semibold : .regular)
            .padding(.horizontal, 14)
            .frame(minWidth: 84, minHeight: 32)
            .foregroundStyle(BrowserPalette.rgb(primary ? values.primaryLabel : values.label))
            .background(BrowserPalette.rgb(primary ? values.primaryFill : values.fill), in: shape)
            .overlay(shape.strokeBorder(line, lineWidth: 1).allowsHitTesting(false))
            .contentShape(shape)
            .opacity(enabled ? (configuration.isPressed ? 0.85 : 1) : 0.5)
    }
}

struct BrowserButtonStyle: ButtonStyle {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.isEnabled) private var enabled
    var selected = false
    var bordered = false
    /// The sidebar's rows sit 24 px apart (round 107.1, catalog entry 10), so its buttons do too.
    var compact = false

    func makeBody(configuration: Configuration) -> some View {
        let palette = BrowserPalette(scheme)
        let shape = RoundedRectangle(cornerRadius: 6)
        configuration.label
            .padding(.horizontal, 7)
            .frame(minHeight: compact ? 24 : 30)
            .contentShape(shape)
            .background(selected ? palette.selected : (configuration.isPressed ? palette.hover : .clear), in: shape)
            .overlay {
                if selected || bordered {
                    shape.strokeBorder(selected ? palette.signal : palette.controlLine, lineWidth: 1)
                        .allowsHitTesting(false)
                }
            }
            .foregroundStyle(enabled ? palette.ink : palette.muted)
    }
}
