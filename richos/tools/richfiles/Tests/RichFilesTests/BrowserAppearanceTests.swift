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
}
