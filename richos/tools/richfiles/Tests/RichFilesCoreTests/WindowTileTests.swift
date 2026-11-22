import CoreGraphics
import RichFilesCore
import XCTest

final class WindowTileTests: XCTestCase {
    func testTilesFillAnOffsetDisplayWithoutGaps() {
        let usable = CGRect(x: -1300, y: -850, width: 1281, height: 801)
        XCTAssertEqual(WindowTile.left.frame(in: usable).union(WindowTile.right.frame(in: usable)), usable)
        XCTAssertEqual(WindowTile.top.frame(in: usable).union(WindowTile.bottom.frame(in: usable)), usable)
        let quarters: [WindowTile] = [.topLeft, .topRight, .bottomLeft, .bottomRight]
        let frames = quarters.map { $0.frame(in: usable) }
        XCTAssertEqual(frames.reduce(CGRect.null) { $0.union($1) }, usable)
        XCTAssertEqual(frames.reduce(CGFloat.zero) { $0 + $1.width * $1.height }, usable.width * usable.height)
        XCTAssertEqual(frames[2].origin, CGPoint(x: usable.minX, y: floor(usable.midY)))
    }

    func testSnapTargetsUseTheDisplayUnderThePointer() {
        let display = CGRect(x: -1440, y: -900, width: 1440, height: 900)
        XCTAssertEqual(WindowTile.atEdge(CGPoint(x: -1439, y: -894), of: display), .topLeft)
        XCTAssertEqual(WindowTile.atEdge(CGPoint(x: -2, y: -300), of: display), .right)
        XCTAssertEqual(WindowTile.atEdge(CGPoint(x: -2, y: -2), of: display), .bottomRight)
        XCTAssertNil(WindowTile.atEdge(CGPoint(x: -720, y: -450), of: display))
        XCTAssertNil(WindowTile.atEdge(CGPoint(x: 1, y: -300), of: display))
    }
}
