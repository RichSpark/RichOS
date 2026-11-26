import Foundation
import XCTest

/// macOS draws a sheet's default button (`.keyboardShortcut(.defaultAction)` with no style of its
/// own) and a `.borderedProminent` button in the system blue, where even a pure white label
/// reaches only 3.75:1 in the light theme (sampled RGB 12, 129, 255 on the Open With chooser),
/// below WCAG AA's 4.5:1. Every RichFiles sheet and dialog draws its default button with
/// `SheetButtonStyle(primary: true)` instead, whose colors `BrowserAppearanceTests` computes.
final class SheetButtonColorTests: XCTestCase {
    /// The SwiftUI targets of the RichFiles app. RichGlance and the window-gesture helper are
    /// AppKit apps with no SwiftUI button style to take.
    private static let targets = ["RichFiles", "RichFilesApp", "RichFilesIntegration"]

    func testNoSheetOrDialogButtonIsDrawnInTheSystemBlue() throws {
        let sources = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources")
        let defaultAction = try NSRegularExpression(pattern: #"keyboardShortcut\(\s*(KeyboardShortcut)?\.defaultAction"#)
        let prominent = try NSRegularExpression(pattern: #"\.borderedProminent\b|BorderedProminentButtonStyle"#)
        var scanned = 0
        var blue: [String] = []
        for target in Self.targets {
            let folder = sources.appendingPathComponent(target)
            let files = try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
                .filter { $0.pathExtension == "swift" }
            for file in files {
                let lines = try String(contentsOf: file, encoding: .utf8).components(separatedBy: "\n")
                for (index, line) in lines.enumerated() {
                    let code = Self.withoutComment(line)
                    let range = NSRange(code.startIndex..., in: code)
                    let site = "\(target)/\(file.lastPathComponent):\(index + 1): \(line.trimmingCharacters(in: .whitespaces))"
                    if prominent.firstMatch(in: code, range: range) != nil { blue.append(site) }
                    // A default action is styled where it is declared, so the style sits on its line
                    // (the `sheetDefaultAction()` modifier is that line, used everywhere else).
                    if defaultAction.firstMatch(in: code, range: range) != nil,
                       !code.contains(".buttonStyle(SheetButtonStyle(primary: true))") {
                        blue.append(site)
                    }
                }
                scanned += 1
            }
        }
        XCTAssertGreaterThan(scanned, 20, "the scan found the RichFiles sources at \(sources.path)")
        XCTAssertEqual(blue, [], "drawn in the system blue (use .sheetDefaultAction() or SheetButtonStyle(primary: true)):\n"
                       + blue.joined(separator: "\n"))
    }

    /// The code part of a line: a `//` comment is dropped unless it sits inside a string.
    private static func withoutComment(_ line: String) -> String {
        var inString = false
        var previous: Character = " "
        for (offset, character) in line.enumerated() {
            if character == "\"" && previous != "\\" { inString.toggle() }
            if !inString && character == "/" && previous == "/" {
                return String(line.prefix(offset - 1))
            }
            previous = character
        }
        return line
    }
}
