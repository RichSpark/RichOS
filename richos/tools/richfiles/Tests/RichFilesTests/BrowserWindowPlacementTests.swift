import AppKit
@testable import RichFiles
import XCTest

final class BrowserWindowPlacementTests: XCTestCase {
    @MainActor
    func testClosedSecondaryWindowKeepsItsOwnFrameAcrossReopenAndRelaunch() throws {
        let domain = "RichFiles.WindowPlacementTests.\(UUID())"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let placement = BrowserWindowPlacement(defaults: defaults)
        let primary = placement.claim(.browser, preferred: -1)
        let secondary = placement.claim(.browser, preferred: -1)
        let folder = placement.claim(.folder, preferred: -1)
        let first = NSRect(x: 20, y: 40, width: 800, height: 500)
        let second = NSRect(x: -800, y: 60, width: 780, height: 600)
        placement.save(first, in: primary)
        placement.save(second, in: secondary)
        placement.save(first.offsetBy(dx: 160, dy: 80), in: folder)
        placement.release(secondary)

        let reopened = placement.claim(.browser, preferred: -1)
        XCTAssertEqual(reopened, secondary)
        XCTAssertEqual(placement.savedFrame(for: reopened), second)
        XCTAssertEqual(placement.savedFrame(for: primary), first)

        let nextLaunch = BrowserWindowPlacement(defaults: defaults)
        XCTAssertEqual(nextLaunch.savedFrame(for: nextLaunch.claim(.browser, preferred: 1)), second)
        XCTAssertEqual(nextLaunch.savedFrame(for: nextLaunch.claim(.browser, preferred: -1)), first)
        XCTAssertEqual(nextLaunch.savedFrame(for: nextLaunch.claim(.folder, preferred: 0)),
                       first.offsetBy(dx: 160, dy: 80))
    }

    @MainActor
    func testRestoredFrameFitsAvailableDisplayWithoutMovingFullyVisibleWindows() {
        let main = NSRect(x: 0, y: 0, width: 1440, height: 900)
        let second = NSRect(x: -1920, y: 0, width: 1920, height: 1080)
        let frame = NSRect(x: -1700, y: 300, width: 1000, height: 720)
        XCTAssertEqual(BrowserWindowPlacement.fit(frame, to: [main, second], fallback: main), frame)
        XCTAssertEqual(BrowserWindowPlacement.fit(frame, to: [main], fallback: main),
                       NSRect(x: 220, y: 90, width: 1000, height: 720))
        let obscuredTitleBar = NSRect(x: 500, y: 800, width: 900, height: 500)
        XCTAssertEqual(BrowserWindowPlacement.fit(obscuredTitleBar, to: [main], fallback: main),
                       NSRect(x: 500, y: 400, width: 900, height: 500))
    }

    @MainActor
    func testKeepsWindowsSpanningDisplaysOrAnEdgeWhenTheTitleBarIsReachable() {
        let main = NSRect(x: 0, y: 0, width: 1440, height: 900)
        let second = NSRect(x: 1440, y: 0, width: 1920, height: 1080)
        let spanning = NSRect(x: 1000, y: 100, width: 1000, height: 600)
        let beyondBottom = NSRect(x: 20, y: -25, width: 800, height: 600)
        XCTAssertEqual(BrowserWindowPlacement.fit(spanning, to: [main, second], fallback: main), spanning)
        XCTAssertEqual(BrowserWindowPlacement.fit(beyondBottom, to: [main], fallback: main), beyondBottom)
    }
}
