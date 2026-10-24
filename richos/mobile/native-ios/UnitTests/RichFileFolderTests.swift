import Foundation
import Testing
@testable import RichOSCore
@testable import RichOSNative

/// A file Rich attached comes over into the app's own folder for kept files (`Boot.richFilesDirectory`)
/// on iOS. On the test iPhone (2026-10-22) every tap on a card ended "Your Mac could not send this
/// file" with no request leaving the phone: making the folder failed with NSCocoaErrorDomain 513
/// (POSIX 13, permission denied) under a root-owned `Application Support`. This runs the core's
/// download on iOS, into the folder the app really uses, with the Mac played by stand-ins.
@Suite("Rich file folder on iOS")
struct RichFileFolderTests {
    actor FileStream: EventStreamTransport {
        func open(_ request: HTTPRequest, origin: String) async throws -> (response: HTTPResponse, bytes: AsyncThrowingStream<Data, Error>) {
            let bytes = AsyncThrowingStream<Data, Error> { c in
                c.yield(Data("%PDF-1.4 ".utf8)); c.yield(Data("counteroffer".utf8)); c.finish()
            }
            return (HTTPResponse(status: 200), bytes)
        }
    }

    actor ProbeMac: HTTPTransport {
        func send(_ request: HTTPRequest, origin: String) async throws -> HTTPResponse {
            HTTPResponse(status: 200, headers: ["X-RichOS-Challenge": "Xqra0YQOgVJ9WLmQy9eYs1LSoxDt1Ggv"])
        }
    }

    actor Arrival {
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

    /// Inside the app's own folder (`LocalOnlyStorage.appRoot`, `Application Support/RichOS`), like
    /// `Attachments/` and `Recordings/`: kept out of iCloud and Finder backups (security review I-2), and
    /// writable on the test iPhone, whose `Application Support` itself belongs to root after the
    /// start-time measurement seeds the app's state (perf/ios.py `DeviceState`). Beside it, as
    /// `Application Support/rich-files`, the folder could not be made there (EACCES, 2026-10-22).
    @Test func theFolderIsInsideTheAppsOwnFolder() async throws {
        let dir = try #require(await Boot.richFilesDirectory())
        #expect(dir.path.hasPrefix(LocalOnlyStorage.appRoot.path + "/"), "\(dir.path)")
    }

    @Test func aFileComesOverIntoTheAppsOwnFolder() async throws {
        let dir = try #require(await Boot.richFilesDirectory(), "the app names a folder for kept files")
        let id = "rf_test_\(UUID().uuidString.prefix(8).lowercased())"
        defer { try? FileManager.default.removeItem(at: dir.appendingPathComponent(id)) }
        let identities = MemoryIdentityStore()
        _ = await identities.signer(for: "https://mac.example")
        let network = NetworkEffects(transport: ProbeMac(), stream: FileStream(), identities: identities, richFiles: dir)
        let arrival = Arrival()
        await network.setSink { await arrival.take($0) }
        var s = AppState()
        s.pairing = .paired
        s.mac = MacLink(origin: "https://mac.example", route: .connect, deviceID: "dev_8d4c57b7ff82", threadID: "thr_5c1e")
        s.linkOpen = true
        let now = await network.handle(.fetchRichFile(id: id, threadID: "thr_5c1e", name: "Acme-counteroffer.pdf", total: 21), state: s)
        #expect(now.isEmpty, "the download runs on its own task: \(now)")
        let kept = dir.appendingPathComponent(id).appendingPathComponent("Acme-counteroffer.pdf")
        #expect(await arrival.wait() == .richFileArrived(id: id, path: kept.path))
        #expect(try Data(contentsOf: kept) == Data("%PDF-1.4 counteroffer".utf8))
    }
}
