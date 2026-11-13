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
    }

    private static func rgb(_ value: UInt32) -> Color {
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

    static func heading(_ size: CGFloat) -> Font {
        newsreader ? .custom("Newsreader16pt-Regular", size: size) : .system(size: size, design: .serif)
    }

    private static func register(_ file: String, face: String) -> Bool {
        let bundles = [Bundle.main, Bundle(url: RichFilesInstallation.embeddedApplication)].compactMap { $0 }
        guard let url = bundles.lazy.compactMap({ $0.url(forResource: file, withExtension: "ttf") }).first else {
            return false
        }
        CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
        let font = CTFontCreateWithName(face as CFString, 16, nil)
        return CTFontCopyPostScriptName(font) as String == face
    }
}

struct BrowserButtonStyle: ButtonStyle {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.isEnabled) private var enabled
    var selected = false
    var bordered = false

    func makeBody(configuration: Configuration) -> some View {
        let palette = BrowserPalette(scheme)
        let shape = RoundedRectangle(cornerRadius: 6)
        configuration.label
            .padding(.horizontal, 7)
            .frame(minHeight: 30)
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
