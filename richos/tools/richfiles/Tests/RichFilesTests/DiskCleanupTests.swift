import AppKit
import XCTest
@testable import RichFiles

final class DiskCleanupTests: XCTestCase {
    @MainActor
    func testWheelReturnsOneLevelPerGestureEvenWithMomentum() throws {
        let view = CleanupRingView(frame: NSRect(x: 0, y: 0, width: 640, height: 560))
        var levels = 0
        view.update(DiskCleanupRing(slices: [], raised: true, dark: true, selected: nil, resetZoom: UUID(),
            available: "17.8 GB", allocation: "", reduceMotion: true, onSelect: { _ in }, onOpen: { _ in },
            onUp: { levels += 1 }, onPreview: { _ in }, onReview: { _ in }, onZoom: { _, _ in }))

        func send(_ delta: Int32, phase: NSEvent.Phase = [], momentum: Int64 = 0) throws {
            let event = try XCTUnwrap(CGEvent(scrollWheelEvent2Source: nil, units: .pixel,
                                               wheelCount: 1, wheel1: delta, wheel2: 0, wheel3: 0))
            event.flags = [.maskControl]
            event.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
            event.setIntegerValueField(.scrollWheelEventScrollPhase, value: Int64(phase.rawValue))
            event.setIntegerValueField(.scrollWheelEventMomentumPhase, value: momentum)
            view.scrollWheel(with: try XCTUnwrap(NSEvent(cgEvent: event)))
        }

        try send(12, phase: .began)
        try send(12, phase: .changed)
        try send(0, phase: .ended)
        try send(-14, phase: .began)
        for delta: Int32 in [-18, -16, -12, -10] { try send(delta, phase: .changed) }
        try send(0, phase: .ended)
        try send(-11, momentum: 1)
        try send(-9, momentum: 2)
        XCTAssertEqual(levels, 1)

        try send(-12, phase: .began)
        XCTAssertEqual(levels, 2)
    }
}
