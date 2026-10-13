import XCTest
@testable import RichOSCompanionCore

/// The microphone restarted on a new input device inside a call (a headset plugged in, the default
/// input switched): the recording's two sides must stay paired frame for frame across the gap.
///
/// Measured on the installed app before this existed (richos-hq
/// docs/verification/2026-10-13-granola-call-end-rule/run1): after a default-input switch the
/// microphone never came back. Restarting it is half the fix; the other half is that the time it
/// missed becomes silence on its side, or every later word of the CEO's sits late against the
/// other side. Each test puts one impulse on BOTH sides at the same moment after the gap and reads
/// the WAV back: the two impulses must land on the same frame.
final class MicRestartTests: XCTestCase {

    // MARK: - MicTimeline

    func testOrdinaryBuffersAreNeverPadded() {
        var t = MicTimeline()
        XCTAssertEqual(t.buffer(startSeconds: 10.0, seconds: 0.1, afterRestart: false), 0)
        // A late ordinary buffer (clock jitter) is not a gap: no drift from padding jitter.
        XCTAssertEqual(t.buffer(startSeconds: 10.13, seconds: 0.1, afterRestart: false), 0)
    }

    func testFirstBufferAfterARestartReportsWhatWasMissed() {
        var t = MicTimeline()
        _ = t.buffer(startSeconds: 10.0, seconds: 0.1, afterRestart: false) // ends at 10.1
        XCTAssertEqual(t.buffer(startSeconds: 10.45, seconds: 0.1, afterRestart: true), 0.35, accuracy: 1e-9)
        // The restarted input's later buffers are ordinary again.
        XCTAssertEqual(t.buffer(startSeconds: 10.7, seconds: 0.1, afterRestart: false), 0)
    }

    func testOverlapAndFirstEverBufferMissNothing() {
        var t = MicTimeline()
        XCTAssertEqual(t.buffer(startSeconds: 5.0, seconds: 0.1, afterRestart: true), 0) // nothing before it
        XCTAssertEqual(t.buffer(startSeconds: 5.05, seconds: 0.1, afterRestart: true), 0) // overlaps
    }

    // MARK: - SessionWriter alignment across a restart

    private let rate = 1_000 // frames per second: 1 frame = 1 ms, so the arithmetic reads plainly
    private let t0: Int64 = 1_700_000_000_000

    private func makeWriter(_ clock: @escaping () -> Int64) throws -> (SessionWriter, String) {
        let tmp = NSTemporaryDirectory() + "richos-mic-restart-\(UUID().uuidString)"
        let params = SessionContract.Params(
            sessionId: SessionContract.sessionDirName(startedAt: t0, platformId: "system", slug: "call"),
            startedAt: t0, sampleRate: rate, chunkMs: 3000, micEnabled: true,
            captureTarget: "system", platformId: "system", platformLabel: "Desktop call",
            platformSlug: "call", companionVersion: "test", processHint: nil)
        let writer = SessionWriter(rootZone: tmp, params: params, clock: clock)
        try writer.start()
        return (writer, tmp)
    }

    /// 500 frames of silence with one impulse at frame 100.
    private func marked() -> [Float] {
        var block = [Float](repeating: 0, count: 500)
        block[100] = 0.9
        return block
    }

    /// The frame each side's impulse landed on, read back from the WAV.
    private func impulseFrames(_ writer: SessionWriter) throws -> (mic: Int, system: Int) {
        let pcm = try WavReader.read(path: (writer.dir as NSString).appendingPathComponent(SessionContract.audioPartFile(0)))
        func peak(_ ch: [Float]) -> Int { ch.indices.max(by: { abs(ch[$0]) < abs(ch[$1]) }) ?? -1 }
        return (peak(pcm.channelsData[0]), peak(pcm.channelsData[1]))
    }

    /// One second of both sides, then a 250 ms gap on the microphone only (the system side keeps
    /// arriving), then both sides carry the same impulse. `fill` uses the restart path.
    private func shortGap(fill: Bool) throws -> (mic: Int, system: Int, pad: Int) {
        var clock = t0
        let (writer, tmp) = try makeWriter { clock }
        defer { try? FileManager.default.removeItem(atPath: tmp) }
        clock += 1_000
        writer.pushMic([Float](repeating: 0, count: 1_000)); writer.pushSystem([Float](repeating: 0, count: 1_000))
        try writer.pump()
        for _ in 0..<5 { // 250 ms of the call's side alone, pumped as the 10 Hz pump does
            clock += 50
            writer.pushSystem([Float](repeating: 0, count: 50))
            try writer.pump()
        }
        let pad = fill ? writer.pushMic(marked(), afterGapOf: 0.25) : { writer.pushMic(marked()); return 0 }()
        writer.pushSystem(marked())
        try writer.pump()
        try writer.close()
        let at = try impulseFrames(writer)
        return (at.mic, at.system, pad)
    }

    func testARestartGapKeepsBothSidesOnTheSameFrame() throws {
        let filled = try shortGap(fill: true)
        XCTAssertEqual(filled.pad, 250)
        XCTAssertEqual(filled.system, 1_000 + 250 + 100)
        XCTAssertEqual(filled.mic, filled.system, "the CEO's side must line up with the call's side after the restart")

        // Control: the same gap without the fill puts the CEO's side 250 ms early for the rest of
        // the call. This is what the test would catch if the fill were removed.
        let unfilled = try shortGap(fill: false)
        XCTAssertEqual(unfilled.system - unfilled.mic, 250)
    }

    /// A restart that took longer than the starve threshold: the failover has already written
    /// silence on the microphone side for most of the gap. The fill adds only the rest; filling the
    /// whole gap again would put the CEO's side 2 s late.
    func testFailoverSilenceCountsTowardTheGap() throws {
        var clock = t0
        let (writer, tmp) = try makeWriter { clock }
        defer { try? FileManager.default.removeItem(atPath: tmp) }
        writer.starveThresholdMs = 150
        clock += 1_000
        writer.pushMic([Float](repeating: 0, count: 1_000)); writer.pushSystem([Float](repeating: 0, count: 1_000))
        try writer.pump()
        for _ in 0..<20 { // 2 s of the call's side alone; the failover silence-fills the microphone
            clock += 100
            writer.pushSystem([Float](repeating: 0, count: 100))
            try writer.pump()
        }
        XCTAssertEqual(writer.lastDecision?.micStarved, true)
        clock += 50 // 50 ms more arrives and is not pumped before the microphone is back
        writer.pushSystem([Float](repeating: 0, count: 50))
        let pad = writer.pushMic(marked(), afterGapOf: 2.05)
        writer.pushSystem(marked())
        try writer.pump()
        try writer.close()

        // The failover wrote 2 s of microphone silence (the first pump, 100 ms in, was under the
        // 150 ms threshold and wrote nothing; the second wrote both 100 ms blocks), so the fill
        // adds only the 50 ms nobody wrote yet, never the whole 2.05 s again.
        XCTAssertEqual(pad, 50)
        let at = try impulseFrames(writer)
        XCTAssertEqual(at.system, 1_000 + 2_050 + 100)
        XCTAssertEqual(at.mic, at.system)
    }
}
