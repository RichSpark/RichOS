import Foundation
import Testing
@testable import RichOSCore

/// A file Rich put in front of the user, on the iPhone (round 36 screen 3): the Mac's `file` row, a
/// tap that brings the bytes over on ONE signed `GET /api/messages?kind=file` read in parts, the copy
/// kept for the next tap, the Mac's "gone" said, the wait while the Mac is out of reach, and × stopping
/// it. Android's `RichFileTest`, the same rules.
@Suite struct RichFileTests {
    static let fileRow = #"{"id":"rf_1","thread_id":"thr_5c1e","cursor":2,"role":"rich","kind":"file","text":"Acme-counteroffer.pdf","complete":true,"file":{"id":"rf_1","name":"Acme-counteroffer.pdf","ext":"PDF","type":"PDF","meta":"PDF · 21 bytes","bytes":21,"exists":true}}"#

    /// The Mac's answer to the file's GET, in parts, ending when the last part is sent.
    actor FileStream: EventStreamTransport {
        let status: Int
        let parts: [String]
        private(set) var opened: [HTTPRequest] = []
        init(status: Int = 200, parts: [String]) { self.status = status; self.parts = parts }
        func open(_ request: HTTPRequest, origin: String) async throws -> (response: HTTPResponse, bytes: AsyncThrowingStream<Data, Error>) {
            opened.append(request)
            let parts = self.parts
            let bytes = AsyncThrowingStream<Data, Error> { c in
                for p in parts { c.yield(Data(p.utf8)) }
                c.finish()
            }
            return (HTTPResponse(status: status), bytes)
        }
    }

    /// A Mac that answers the challenge probe; everything else comes through the stream.
    actor ProbeMac: HTTPTransport {
        func send(_ request: HTTPRequest, origin: String) async throws -> HTTPResponse {
            HTTPResponse(status: 200, headers: ["X-RichOS-Challenge": "Xqra0YQOgVJ9WLmQy9eYs1LSoxDt1Ggv"])
        }
    }

    func paired(online: Bool = true) throws -> AppState {
        var s = AppState()
        s.pairing = .paired
        s.mac = MacLink(origin: "https://mac.example", route: .connect, deviceID: "dev_8d4c57b7ff82", threadID: "thr_5c1e")
        s.linkOpen = online
        s.messages = [try CoreJSON.decode(StreamRow.self, from: Data(Self.fileRow.utf8)).message]
        return s
    }

    @Test func theRowCarriesTheCardAndAnIdNeverAPath() throws {
        let m = try CoreJSON.decode(StreamRow.self, from: Data(Self.fileRow.utf8)).message
        #expect(m.author == .rich)
        #expect(m.richFile?.id == "rf_1")
        #expect(m.richFile?.meta == "PDF · 21 bytes")
        #expect(RichFileRule.path(fileID: "rf_1", threadID: "thr_5c1e") == "/api/messages?kind=file&file_id=rf_1&thread_id=thr_5c1e")
        #expect(RichFileRule.progress(received: 62_000, total: 184_000) == "62 of 184 KB from your Mac")
    }

    @Test func aTapAsksForTheFileAndTheArrivalOpensIt() throws {
        var s = try paired()
        var r = Reducer.reduce(s, .openRichFile(id: "rf_1"))
        #expect(r.effects == [.findRichFile(id: "rf_1", name: "Acme-counteroffer.pdf")], "the phone's own copy first")
        #expect(r.state.fileFetches["rf_1"]?.state == .finding)
        r = Reducer.reduce(r.state, .richFileNotKept(id: "rf_1"))
        #expect(r.effects.contains(.fetchRichFile(id: "rf_1", threadID: "thr_5c1e", name: "Acme-counteroffer.pdf", total: 21)))
        #expect(r.state.fileFetches["rf_1"]?.state == .coming)
        s = r.state
        s = Reducer.reduce(s, .richFileProgress(id: "rf_1", received: 10)).state
        #expect(s.fileFetches["rf_1"]?.received == 10)
        s = Reducer.reduce(s, .richFileArrived(id: "rf_1", path: "/kept/rf_1/Acme-counteroffer.pdf")).state
        #expect(s.fileFetches["rf_1"]?.state == .kept)
        #expect(s.fileFetches["rf_1"]?.open == true, "the viewer opens")
        s = Reducer.reduce(s, .richFileViewed(id: "rf_1")).state
        #expect(s.fileFetches["rf_1"]?.open == false)
        r = Reducer.reduce(s, .openRichFile(id: "rf_1"))
        #expect(r.effects.isEmpty, "the kept copy opens without asking the Mac")
        #expect(r.state.fileFetches["rf_1"]?.open == true)
    }

    @Test func withTheMacOutOfReachItWaitsAndAsksWhenTheLinkOpens() throws {
        var r = Reducer.reduce(try paired(online: false), .openRichFile(id: "rf_1"))
        r = Reducer.reduce(r.state, .richFileNotKept(id: "rf_1"))
        #expect(r.state.fileFetches["rf_1"]?.state == .waiting)
        #expect(!r.effects.contains { if case .fetchRichFile = $0 { return true }; return false })
        r = Reducer.reduce(r.state, .connected(at: 5))
        #expect(r.state.fileFetches["rf_1"]?.state == .coming)
        #expect(r.effects.contains(.fetchRichFile(id: "rf_1", threadID: "thr_5c1e", name: "Acme-counteroffer.pdf", total: 21)))
    }

    @Test func xStopsItAndALateArrivalLeavesNothingBehind() throws {
        var r = Reducer.reduce(try paired(), .openRichFile(id: "rf_1"))
        r = Reducer.reduce(r.state, .richFileNotKept(id: "rf_1"))
        r = Reducer.reduce(r.state, .stopRichFile(id: "rf_1"))
        #expect(r.effects.contains(.stopRichFile(id: "rf_1")))
        #expect(r.state.fileFetches["rf_1"] == nil)
        r = Reducer.reduce(r.state, .richFileArrived(id: "rf_1", path: "/kept/x"))
        #expect(r.state.fileFetches["rf_1"] == nil)
    }

    @Test func theMacsGoneIsSaid() throws {
        var r = Reducer.reduce(try paired(), .openRichFile(id: "rf_1"))
        r = Reducer.reduce(r.state, .richFileFailed(id: "rf_1", reason: RichFileRule.gone))
        #expect(r.state.fileFetches["rf_1"]?.state == .failed)
        #expect(r.state.fileFetches["rf_1"]?.reason == "No longer on this Mac. It was moved or deleted.")
    }

    @Test func theBytesComeOverOnOneSignedRequestAndAreKept() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("richfiles-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let identities = MemoryIdentityStore()
        _ = await identities.signer(for: "https://mac.example")
        let stream = FileStream(parts: ["%PDF-1.4 ", "counteroffer"])
        let network = NetworkEffects(transport: ProbeMac(), stream: stream, identities: identities, clock: FixedClock(ms: 100), richFiles: dir)
        let arrived = ArrivalBox()
        await network.setSink { action in await arrived.take(action) }
        let s = try paired()
        let now = await network.handle(.fetchRichFile(id: "rf_1", threadID: "thr_5c1e", name: "Acme-counteroffer.pdf", total: 21), state: s)
        #expect(now.isEmpty, "the download runs on its own task")
        let action = await arrived.wait()
        let kept = dir.appendingPathComponent("rf_1").appendingPathComponent("Acme-counteroffer.pdf")
        #expect(action == .richFileArrived(id: "rf_1", path: kept.path))
        #expect(try Data(contentsOf: kept) == Data("%PDF-1.4 counteroffer".utf8))
        let opened = await stream.opened
        #expect(opened.first?.target == "/api/messages?kind=file&file_id=rf_1&thread_id=thr_5c1e")
        #expect(opened.first?.headers["Authorization"] != nil, "signed in the header")
        // A second tap finds the kept copy and asks nothing.
        let again = await network.handle(.fetchRichFile(id: "rf_1", threadID: "thr_5c1e", name: "Acme-counteroffer.pdf", total: 21), state: s)
        #expect(again == [.richFileArrived(id: "rf_1", path: kept.path)])
        #expect(await stream.opened.count == 1)
    }

    /// A Mac that must never be asked: the relaunch cases below open the phone's own copy.
    actor SilentMac: HTTPTransport, EventStreamTransport {
        private(set) var asked = 0
        func send(_ request: HTTPRequest, origin: String) async throws -> HTTPResponse { asked += 1; throw URLError(.notConnectedToInternet) }
        func open(_ request: HTTPRequest, origin: String) async throws -> (response: HTTPResponse, bytes: AsyncThrowingStream<Data, Error>) {
            asked += 1; throw URLError(.notConnectedToInternet)
        }
    }

    /// Runs each effect through the network and each action it answers through the reducer.
    func settle(_ r: (state: AppState, effects: [Effect]), _ network: NetworkEffects) async -> (state: AppState, effects: [Effect]) {
        var s = r.state
        var all: [Effect] = []
        var queue = r.effects
        while !queue.isEmpty {
            let effect = queue.removeFirst()
            for action in await network.handle(effect, state: s) {
                let next = Reducer.reduce(s, action)
                s = next.state; all += next.effects; queue += next.effects
            }
        }
        return (s, all)
    }

    /// A file that came over before, a relaunch (only what is saved survives), then a tap: the kept
    /// copy opens without the Mac, whether the Mac is out of reach, says the original is gone, or both.
    /// Review rv-20261021T100557Z-21af5639-503f: the tap used to wait for, or fail on, the Mac first.
    @Test(arguments: [(false, true), (true, false), (false, false)])
    func afterARelaunchTheKeptCopyOpensWithoutTheMac(online: Bool, onMac: Bool) async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("richfiles-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let kept = dir.appendingPathComponent("rf_1").appendingPathComponent("Acme-counteroffer.pdf")
        try FileManager.default.createDirectory(at: kept.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("%PDF-1.4 counteroffer".utf8).write(to: kept)
        var before = try paired(online: online)
        before.messages[0].richFile?.exists = onMac
        before.fileFetches["rf_1"] = FileFetch(id: "rf_1", state: .kept, received: 21, total: 21, path: kept.path)
        // The relaunch: only what is saved comes back, and the app starts with no link open.
        var relaunched = try AppState(restoring: CoreJSON.decode(AppState.Persisted.self, from: CoreJSON.encode(before.persisted)))
        relaunched.linkOpen = online
        #expect(relaunched.fileFetches.isEmpty, "what the tap knew is gone after a relaunch")
        #expect(relaunched.messages[0].richFile?.exists == onMac)

        let mac = SilentMac()
        let network = NetworkEffects(transport: mac, stream: mac, identities: MemoryIdentityStore(), clock: FixedClock(ms: 100), richFiles: dir)
        let tap = Reducer.reduce(relaunched, .openRichFile(id: "rf_1"))
        #expect(tap.effects == [.findRichFile(id: "rf_1", name: "Acme-counteroffer.pdf")], "the phone's own copy first")
        let r = await settle((tap.state, tap.effects), network)
        #expect(r.state.fileFetches["rf_1"]?.state == .kept)
        #expect(r.state.fileFetches["rf_1"]?.path == kept.path)
        #expect(r.state.fileFetches["rf_1"]?.open == true, "the viewer opens")
        #expect(!r.effects.contains { if case .fetchRichFile = $0 { return true }; return false })
        #expect(await mac.asked == 0, "the Mac is never asked")
    }

    /// No copy on this phone and the original gone on the Mac: the Mac's own sentence.
    @Test func noKeptCopyAndGoneOnTheMacSaysSo() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("richfiles-\(UUID().uuidString)")
        var s = try paired()
        s.messages[0].richFile?.exists = false
        let mac = SilentMac()
        let network = NetworkEffects(transport: mac, stream: mac, identities: MemoryIdentityStore(), clock: FixedClock(ms: 100), richFiles: dir)
        let tap = Reducer.reduce(s, .openRichFile(id: "rf_1"))
        let r = await settle((tap.state, tap.effects), network)
        #expect(r.state.fileFetches["rf_1"]?.state == .failed)
        #expect(r.state.fileFetches["rf_1"]?.reason == RichFileRule.gone)
    }

    /// The first terminal action the sink receives.
    actor ArrivalBox {
        private var got: Action?
        private var waiting: CheckedContinuation<Action, Never>?
        func take(_ action: Action) {
            if case .richFileProgress = action { return }
            if let waiting { self.waiting = nil; waiting.resume(returning: action) } else { got = got ?? action }
        }
        func wait() async -> Action {
            if let got { return got }
            return await withCheckedContinuation { waiting = $0 }
        }
    }
}
