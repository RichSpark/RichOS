import CoreGraphics
import XCTest
import RichFilesCore

final class GlanceCaptureSelectionTests: XCTestCase {
    func testAimingNudgeAndDragNudgeSurviveMouseDownAndRelease() {
        let start = CGPoint(x: 20, y: 10)
        var selection = GlanceCaptureSelection(width: 400, height: 300, pointer: start)
        selection.nudge(dx: 1, dy: 1)
        selection.mouseDown(at: start)
        XCTAssertEqual(selection.first, .init(x: 21, y: 11))
        selection.movePointer(to: CGPoint(x: 122, y: 68))
        XCTAssertEqual(selection.crop, CGRect(x: 21, y: 11, width: 101, height: 57))
        selection.nudge(dx: 1, dy: 0)
        selection.mouseUp()
        selection.movePointer(to: CGPoint(x: 200, y: 100))
        XCTAssertEqual(selection.phase, .refining)
        XCTAssertEqual(selection.crop, CGRect(x: 21, y: 11, width: 102, height: 57))
        selection.switchCorner()
        selection.nudge(dx: -1, dy: 0)
        XCTAssertEqual(selection.first, .init(x: 20, y: 11))
        XCTAssertEqual(selection.second, .init(x: 123, y: 68))
        XCTAssertEqual(selection.crop?.width, 103)
    }

    func testKeyboardPlacementCanCrossCornersAndClampsAtEdges() {
        var selection = GlanceCaptureSelection(width: 40, height: 30, pointer: CGPoint(x: 3, y: 10))
        selection.shiftReturn()
        selection.shiftReturn()
        XCTAssertEqual(selection.phase, .placing)
        XCTAssertFalse(selection.canCapture)
        for _ in 0..<8 { selection.nudge(dx: -1, dy: -1) }
        selection.shiftReturn()
        XCTAssertEqual(selection.phase, .refining)
        XCTAssertEqual(selection.target, .init(x: 0, y: 2))
        XCTAssertEqual(selection.crop, CGRect(x: 0, y: 2, width: 3, height: 8))
        selection.switchCorner()
        selection.nudge(dx: -1, dy: 0)
        XCTAssertEqual(selection.crop?.width, 2)
        selection.toggleLens()
        XCTAssertFalse(selection.showsLens)
        XCTAssertTrue(selection.canCapture)
    }

    func testEmptyClickAndStartOverAllowAnotherDrag() {
        let origin = CGPoint(x: 50, y: 75)
        var selection = GlanceCaptureSelection(width: 200, height: 200, pointer: origin)
        selection.mouseDown(at: origin)
        selection.mouseUp()
        XCTAssertNil(selection.crop)

        selection.mouseDown(at: origin)
        selection.movePointer(to: CGPoint(x: 150, y: 125))
        selection.mouseUp()
        selection.switchCorner()
        selection.toggleLens()
        XCTAssertTrue(selection.canCapture)

        let newOrigin = CGPoint(x: 110, y: 110)
        selection.startOver(at: newOrigin)
        XCTAssertNil(selection.crop)
        XCTAssertFalse(selection.showsLens)
        selection.mouseDown(at: newOrigin)
        selection.movePointer(to: CGPoint(x: 90, y: 80))
        selection.mouseUp()
        XCTAssertEqual(selection.crop, CGRect(x: 90, y: 80, width: 20, height: 30))
        XCTAssertEqual(selection.target, .init(x: 90, y: 80))
    }

    func testOnePixelNudgeUsesTheActualFrameScaleAndKeepsTheExactCrop() throws {
        let frame = CGSize(width: 200, height: 200)
        let color = CGColorSpaceCreateDeviceRGB()
        let context = try XCTUnwrap(CGContext(data: nil, width: 200, height: 200, bitsPerComponent: 8,
                                             bytesPerRow: 0, space: color,
                                             bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        for x in 0..<200 {
            context.setFillColor(red: x.isMultiple(of: 2) ? 1 : 0, green: 0,
                                 blue: x.isMultiple(of: 2) ? 0 : 1, alpha: 1)
            context.fill(CGRect(x: x, y: 0, width: 1, height: 200))
        }
        let image = try XCTUnwrap(context.makeImage())
        for scale in [1.0, 2.0] {
            let geometry = GlanceCaptureGeometry(frame: frame, view: CGSize(width: 200 / scale, height: 200 / scale))
            var selection = GlanceCaptureSelection(width: 200, height: 200, pointer: CGPoint(x: 10, y: 12))
            selection.shiftReturn()
            selection.movePointer(to: geometry.sourcePoint(CGPoint(x: 111 / scale, y: 69 / scale)))
            selection.shiftReturn()
            let old = geometry.viewPoint(selection.target)
            XCTAssertEqual(selection.crop, CGRect(x: 10, y: 12, width: 101, height: 57))
            selection.nudge(dx: 1, dy: 0)
            XCTAssertEqual(geometry.viewPoint(selection.target).x - old.x, 1 / scale)
            let rect = try XCTUnwrap(selection.crop)
            let crop = try XCTUnwrap(image.cropping(to: rect))
            XCTAssertEqual(crop.width, 102)
            XCTAssertEqual(crop.height, 57)
            XCTAssertEqual(try pixel(crop, atX: 100), [255, 0, 0, 255])
            XCTAssertEqual(try pixel(crop, atX: 101), [0, 0, 255, 255])
        }
    }

    private func pixel(_ image: CGImage, atX x: Int) throws -> [UInt8] {
        let sample = try XCTUnwrap(image.cropping(to: CGRect(x: x, y: 0, width: 1, height: 1)))
        var rgba = [UInt8](repeating: 0, count: 4)
        try rgba.withUnsafeMutableBytes { bytes in
            let context = try XCTUnwrap(CGContext(data: bytes.baseAddress, width: 1, height: 1,
                                                bitsPerComponent: 8, bytesPerRow: 4,
                                                space: CGColorSpaceCreateDeviceRGB(),
                                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.draw(sample, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        }
        return rgba
    }
}
