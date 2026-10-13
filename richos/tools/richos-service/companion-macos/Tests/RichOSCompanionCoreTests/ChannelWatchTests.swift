import XCTest
@testable import RichOSCompanionCore

/// A side of the recording that stalls is said to the app once, and once when it comes back
/// (second review rv-20261013T090233Z-2230466e-9d08, finding 1).
final class ChannelWatchTests: XCTestCase {

    func testHealthyTicksSayNothing() {
        var w = ChannelWatch()
        for _ in 0..<30 { XCTAssertEqual(w.update(micDown: false, systemDown: false), []) }
    }

    /// The review's fixture: the microphone stops after five ticks while system audio continues.
    /// Before this, 21 red ticks produced zero alarms; now the first one produces exactly one.
    func testAMicrophoneThatStopsIsSaidOnceNotEverySecond() {
        var w = ChannelWatch()
        var changes: [ChannelChange] = []
        for tick in 0..<26 { changes += w.update(micDown: tick >= 5, systemDown: false) }
        XCTAssertEqual(changes, [ChannelChange(channel: .microphone, stalled: true)])
        XCTAssertTrue(changes[0].message.contains("microphone stopped"))
    }

    func testAResumedSideIsSaidAndBothSidesAreIndependent() {
        var w = ChannelWatch()
        XCTAssertEqual(w.update(micDown: false, systemDown: true), [ChannelChange(channel: .systemAudio, stalled: true)])
        XCTAssertEqual(w.update(micDown: true, systemDown: true), [ChannelChange(channel: .microphone, stalled: true)])
        XCTAssertEqual(w.update(micDown: true, systemDown: false), [ChannelChange(channel: .systemAudio, stalled: false)])
        XCTAssertEqual(CaptureChannel.systemAudio.rawValue, "system-audio")
    }

    // End-detection run 2 (2026-10-13): the microphone side was exactly zero for all 195 s, every
    // health row green, and nothing said it.
    func testAMicrophoneGivingExactSilenceIsSaidOnceAfterTenSecondsAndItsReturnOnce() {
        var s = MicSilence()
        var said: [String] = []
        for _ in 0..<30 { if let x = s.update(micPeak: 0, micRunning: true) { said.append(x) } }
        XCTAssertEqual(said, [MicSilence.silent])
        XCTAssertTrue(MicSilence.silent.contains("microphone access may be turned off"))
        XCTAssertEqual(s.update(micPeak: 0.002, micRunning: true), MicSilence.back)
        XCTAssertNil(s.update(micPeak: 0.003, micRunning: true))
    }

    func testAQuietRoomOrAStoppedMicrophoneIsNotSilenceToSay() {
        var s = MicSilence()
        // A real microphone's noise floor is never exactly zero.
        for _ in 0..<30 { XCTAssertNil(s.update(micPeak: 0.0001, micRunning: true)) }
        // Nine zero seconds, then sound: nothing said.
        for _ in 0..<9 { XCTAssertNil(s.update(micPeak: 0, micRunning: true)) }
        XCTAssertNil(s.update(micPeak: 0.01, micRunning: true))
        // A microphone that is not running is ChannelWatch's to say, not this.
        for _ in 0..<30 { XCTAssertNil(s.update(micPeak: 0, micRunning: false)) }
    }
}
