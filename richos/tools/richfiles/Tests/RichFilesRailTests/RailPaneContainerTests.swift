import AppKit
import XCTest
@testable import RichFilesRail

/// The same view layout RailHost.place builds beside the web view, then the state
/// RailHost.hide leaves it in when the user goes back to Chats.
@MainActor
final class RailPaneContainerTests: XCTestCase {
    func testClickInHiddenPaneAreaReachesWebView() {
        let parent = NSView(frame: NSRect(x: 0, y: 0, width: 1200, height: 800))
        let web = NSView(frame: parent.bounds)
        parent.addSubview(web)
        let container = RailPaneContainer()
        container.wantsLayer = true
        parent.addSubview(container, positioned: .above, relativeTo: web)
        let pane = NSView()
        pane.isHidden = true
        container.addSubview(pane)

        // RichFiles shown: place sizes the container to the pane area and shows the pane.
        container.frame = NSRect(x: 300, y: 0, width: 900, height: 800)
        pane.frame = container.bounds
        pane.isHidden = false
        let click = NSPoint(x: 700, y: 400)
        XCTAssertTrue(parent.hitTest(click) === pane, "RichFiles takes clicks while it is shown")

        // Back on Chats: hide hides the pane and drops the tooltip cutout.
        pane.isHidden = true
        container.layer?.mask = nil
        XCTAssertTrue(parent.hitTest(click) === web, "a click where the pane was must reach the web view")
    }
}
