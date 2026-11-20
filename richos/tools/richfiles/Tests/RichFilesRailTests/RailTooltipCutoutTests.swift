import AppKit
import WebKit
import XCTest
@testable import RichFilesRail

/// The hole RailHost cuts in the docked pane is where WebKit draws the rail's hover label.
///
/// The window is laid out as RichOS's is: the web view fills a content view that reaches up
/// beneath the title bar, while the page's CSS viewport (innerWidth x innerHeight) is the part
/// below it, the window's content layout rect. The label's rectangle is what the page reports.
@MainActor
final class RailTooltipCutoutTests: XCTestCase {
    /// A window on a display of the given scale. The window is never put on screen.
    private final class ScaledWindow: NSWindow {
        var scale: CGFloat = 1
        override var backingScaleFactor: CGFloat { scale }
    }

    private struct Layout {
        let window: ScaledWindow
        let web: WKWebView
        let container: NSView
        /// The CSS viewport, as the page sees it: innerWidth and innerHeight.
        let inner: CGSize
    }

    private func layout(width: CGFloat, height: CGFloat, scale: CGFloat) -> Layout {
        let window = ScaledWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: height),
                                  styleMask: [.titled, .closable, .resizable, .fullSizeContentView],
                                  backing: .buffered, defer: true)
        window.scale = scale
        window.isReleasedWhenClosed = false
        let parent = NSView(frame: NSRect(origin: .zero, size: window.frame.size))
        window.contentView = parent
        let web = WKWebView(frame: parent.bounds)
        parent.addSubview(web)
        let container = RailPaneContainer()
        container.wantsLayer = true
        parent.addSubview(container, positioned: .above, relativeTo: web)
        // RichFiles to the right of the 52 px rail, down to the window's bottom edge.
        container.frame = NSRect(x: 52, y: 0, width: parent.bounds.width - 52, height: parent.bounds.height)
        return Layout(window: window, web: web, container: container, inner: window.contentLayoutRect.size)
    }

    /// One label, as the page reports it, checked against where it is on the window.
    private func check(width: CGFloat, height: CGFloat, scale: CGFloat, label css: CGRect,
                       file: StaticString = #filePath, line: UInt = #line) throws {
        let view = layout(width: width, height: height, scale: scale)
        defer { view.window.close() }
        let titleBar = view.window.frame.height - view.window.contentLayoutRect.height
        XCTAssertGreaterThan(titleBar, 20, "the window has a title bar the web view reaches beneath", file: file, line: line)

        let reported = CGRect(x: css.minX / view.inner.width, y: css.minY / view.inner.height,
                              width: css.width / view.inner.width, height: css.height / view.inner.height)
        let cutout = try XCTUnwrap(RailTooltipCutout(label: reported, radius: 7 / view.inner.width,
                                                     arrow: 5 / view.inner.width, webView: view.web,
                                                     container: view.container),
                                   "the label overlaps the pane, so it gets a hole", file: file, line: line)

        // Where the label is, in window coordinates (origin bottom left): css px are points.
        let layoutRect = view.window.contentLayoutRect
        let expected = CGRect(x: layoutRect.minX + css.minX, y: layoutRect.maxY - css.maxY,
                              width: css.width, height: css.height)
        let hole = view.container.convert(cutout.hole, to: nil)
        for (got, want, name) in [(hole.minX, expected.minX, "left"), (hole.minY, expected.minY, "bottom"),
                                  (hole.width, expected.width, "width"), (hole.height, expected.height, "height")] {
            XCTAssertEqual(got, want, accuracy: 0.01,
                           "\(Int(width))x\(Int(height)) at \(Int(scale))x: the hole's \(name) is the label's", file: file, line: line)
        }

        // And the mask itself: the label's middle is cut out, the pane above and below it is not.
        let middle = view.container.convert(CGPoint(x: expected.midX, y: expected.midY), from: nil)
        let above = view.container.convert(CGPoint(x: expected.midX, y: expected.maxY + 10), from: nil)
        let below = view.container.convert(CGPoint(x: expected.midX, y: expected.minY - 10), from: nil)
        XCTAssertFalse(cutout.path.contains(middle, using: .evenOdd), "the label's middle shows through", file: file, line: line)
        XCTAssertTrue(cutout.path.contains(above, using: .evenOdd), "no hole above the label", file: file, line: line)
        XCTAssertTrue(cutout.path.contains(below, using: .evenOdd), "no hole below the label", file: file, line: line)
    }

    // A label part way down the rail, just right of it: the old mapping put its hole about
    // a title bar's height above it.
    private let richCleaner = CGRect(x: 58, y: 197, width: 118, height: 31)

    func testHoleIsTheLabelAtOneX() throws {
        try check(width: 1400, height: 835, scale: 1, label: richCleaner)
    }

    func testHoleIsTheLabelAtTwoX() throws {
        try check(width: 1400, height: 835, scale: 2, label: richCleaner)
    }

    func testHoleIsTheLabelAtOtherWindowSizes() throws {
        // The smallest window RichOS allows, and a tall one; a label near the top and one low down.
        try check(width: 1024, height: 700, scale: 1, label: CGRect(x: 58, y: 57, width: 70, height: 31))
        try check(width: 1880, height: 1150, scale: 2, label: CGRect(x: 58, y: 900, width: 140, height: 31))
    }
}
