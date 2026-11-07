import XCTest
@testable import RichOSCompanionCore

/// `richos-companion watch`: which other processes use the microphone (Granola PRD V4).
final class MicActivityTests: XCTestCase {

    private func p(_ pid: Int32, _ id: String?, input: Bool, output: Bool = false) -> AudioProcess {
        AudioProcess(pid: pid, bundleId: id, runningInput: input, runningOutput: output)
    }

    func testFamilyStripsHelperSuffixes() {
        XCTAssertEqual(AudioProcess.family(of: "com.google.Chrome.helper"), "com.google.Chrome")
        XCTAssertEqual(AudioProcess.family(of: "com.google.Chrome.helper.renderer"), "com.google.Chrome")
        XCTAssertEqual(AudioProcess.family(of: "com.microsoft.edgemac.helper"), "com.microsoft.edgemac")
        XCTAssertEqual(AudioProcess.family(of: "us.zoom.xos"), "us.zoom.xos")
        XCTAssertNil(AudioProcess.family(of: nil))
        XCTAssertNil(AudioProcess.family(of: ""))
    }

    func testFirstReadingReportsWhatIsAlreadyRunningInput() {
        var t = MicActivityTracker(filter: MicActivityFilter())
        let changes = t.update([p(10, "us.zoom.xos", input: true), p(11, "com.apple.Music", input: false, output: true)], at: 0)
        XCTAssertEqual(changes, [.started(p(10, "us.zoom.xos", input: true))])
    }

    func testStopIsReportedWhenInputCloses() {
        var t = MicActivityTracker(filter: MicActivityFilter())
        _ = t.update([p(10, "us.zoom.xos", input: true)], at: 0)
        let changes = t.update([p(10, "us.zoom.xos", input: false, output: true)], at: 0)
        XCTAssertEqual(changes, [.stopped(p(10, "us.zoom.xos", input: true), exited: false)])
    }

    func testChromePlaybackBridgesJoinAndKeepsAMutedCallUntilHangUp() {
        var tracker = MicActivityTracker(filter: MicActivityFilter())
        let idle = p(10, "com.google.Chrome.helper", input: false)
        let muted = p(10, "com.google.Chrome.helper", input: false, output: true)
        let joined = p(10, "com.google.Chrome.helper", input: true, output: true)
        XCTAssertEqual(tracker.update([idle], at: 0), [])
        XCTAssertEqual(tracker.update([muted], at: 1_000), [], "output alone does not prompt")
        XCTAssertEqual(tracker.update([joined], at: 5_000), [.started(joined)])
        XCTAssertEqual(tracker.update([muted], at: 7_000), [], "input reset during join")
        XCTAssertEqual(tracker.update([joined], at: 11_000), [], "Chrome finished joining")
        XCTAssertEqual(tracker.update([muted], at: 20_000), [])
        XCTAssertEqual(tracker.update([muted], at: 50_000), [])
        XCTAssertEqual(tracker.update([joined], at: 60_000), [], "unmuting is the same call")
        XCTAssertEqual(tracker.update([muted], at: 70_000), [])
        XCTAssertEqual(tracker.update([idle], at: 90_000), [.stopped(muted, exited: false)])
    }

    func testChromePlaybackThatPredatesTheCallDoesNotHoldItOpen() {
        var tracker = MicActivityTracker(filter: MicActivityFilter())
        let playback = p(10, "com.google.Chrome.helper", input: false, output: true)
        let joined = p(10, "com.google.Chrome.helper", input: true, output: true)
        XCTAssertEqual(tracker.update([playback], at: 0), [])
        XCTAssertEqual(tracker.update([joined], at: 60_000), [.started(joined)])
        XCTAssertEqual(tracker.update([playback], at: 80_000), [.stopped(joined, exited: false)])
    }

    func testChromePlaybackThatStartsLaterDoesNotHoldTheCallOpen() {
        var tracker = MicActivityTracker(filter: MicActivityFilter())
        let input = p(10, "com.google.Chrome.helper", input: true)
        let both = p(10, "com.google.Chrome.helper", input: true, output: true)
        let playback = p(10, "com.google.Chrome.helper", input: false, output: true)
        XCTAssertEqual(tracker.update([input], at: 0), [.started(input)])
        XCTAssertEqual(tracker.update([both], at: 60_000), [])
        XCTAssertEqual(tracker.update([playback], at: 80_000), [.stopped(both, exited: false)])
    }

    func testStopIsReportedAsExitWhenTheProcessLeavesTheList() {
        var t = MicActivityTracker(filter: MicActivityFilter())
        _ = t.update([p(10, "us.zoom.xos", input: true)], at: 0)
        XCTAssertEqual(t.update([], at: 0), [.stopped(p(10, "us.zoom.xos", input: true), exited: true)])
    }

    func testUnchangedReadingReportsNothing() {
        var t = MicActivityTracker(filter: MicActivityFilter())
        _ = t.update([p(10, "us.zoom.xos", input: true)], at: 0)
        XCTAssertEqual(t.update([p(10, "us.zoom.xos", input: true)], at: 0), [])
    }

    func testOwnProcessesAreExcludedByPidBundleAndFamily() {
        let f = MicActivityFilter(pids: [42], bundleIds: ["com.richos.app"])
        var t = MicActivityTracker(filter: f)
        let changes = t.update([
            p(42, "com.example.anything", input: true),       // the companion, by pid
            p(43, "com.richos.app", input: true),             // voice/dictation, by bundle
            p(44, "com.richos.app.helper", input: true),      // by family
            p(45, "com.google.Chrome.helper", input: true),   // a browser call: reported
        ], at: 0)
        XCTAssertEqual(changes, [.started(p(45, "com.google.Chrome.helper", input: true))])
    }

    func testChangesAreOrderedStoppedThenStartedByPid() {
        var t = MicActivityTracker(filter: MicActivityFilter())
        _ = t.update([p(5, "a", input: true), p(3, "b", input: true)], at: 0)
        let changes = t.update([p(9, "c", input: true), p(7, "d", input: true)], at: 0)
        XCTAssertEqual(changes, [
            .stopped(p(3, "b", input: true), exited: true),
            .stopped(p(5, "a", input: true), exited: true),
            .started(p(7, "d", input: true)),
            .started(p(9, "c", input: true)),
        ])
    }

    func testLineShape() {
        let started = MicActivityTracker.line(.started(p(45, "com.google.Chrome.helper", input: true)), at: 1000, app: "Google Chrome")
        XCTAssertEqual(started.compact(),
            #"{"type":"input","state":"started","at":1000,"pid":45,"bundleId":"com.google.Chrome.helper","family":"com.google.Chrome","app":"Google Chrome"}"#)
        let stopped = MicActivityTracker.line(.stopped(p(10, nil, input: true), exited: true), at: 2000, app: nil)
        XCTAssertEqual(stopped.compact(),
            #"{"type":"input","state":"stopped","exited":true,"at":2000,"pid":10,"bundleId":null,"family":null,"app":null}"#)
    }
}
