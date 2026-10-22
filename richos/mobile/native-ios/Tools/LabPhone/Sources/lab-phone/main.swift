import Foundation
import RichOSCore
import RichOSFixtures

// lab-phone --lab LAB_CACHE --marks FILE [--tries N] [--mode fresh|return] [--home-after S] [--hidden S]
//           [--pose send|wifi-drop] [--offline-at S] [--offline-for S] [--away-before S]
//
//   --lab         the isolated lab's cache (`mobile.mjs lab mac --manual`, fresh, so its pairing window
//                 is open). Refused unless its data directory carries the isolated lab's owner marker,
//                 so this never pairs with a real Mac. A lab started WITHOUT the managed route (its
//                 `mac.json` says `"protocol": "https"`: the Rust listener is TLS with the lab's own CA
//                 and the lab puts a plain-HTTP loopback proxy in front of it) is reached through that
//                 proxy: the production `URLSessionTransport` is handed the proxy's origin in place of
//                 the paired one, and nothing else changes. A managed lab is reached through its route.
//   --marks       a file this appends PHONE_STEP mark lines to ("PAUSE MAC n" one second before Send,
//                 "hidden n" half a second after leaving the screen), for `qa/lab-pause.py --log`.
//   --tries       send-then-Home cycles (default 3).
//   --mode        fresh: each cycle begins with a new launch (a new store, network handler and
//                 URLSession over the same saved state and keys, like a relaunch); return: each begins
//                 after the previous return to the screen (default fresh).
//   --home-after  seconds from Send to leaving the screen (default 2.0, the phone runner's tap-then-Home).
//   --hidden      seconds kept off screen (default 25, the walk's).
//   --pose        send (default): the cycle above. wifi-drop: iPhone re-walk 4's Wi-Fi-drop tries, where
//                 one return took 17.11 s to show the reply: `--offline-at` seconds after leaving the
//                 screen the phone's network goes away (every request started then fails at once, as
//                 URLSession does with Wi-Fi off), "RELEASE MAC n" is marked at that instant (for
//                 `lab-pause.py --cont-at`), and `--offline-for` seconds later the network is back.
//   --offline-at  seconds after Home that the wifi-drop pose turns the network off (default 3.25, the walk's).
//   --offline-for seconds the network stays off (default 3.4, the walk's).
//   --away-before seconds the app is off screen before each try, coming back 5.5 s before "PAUSE MAC n"
//                 (default 0; the walk's runner was away about 11 s, reading Settings, between tries).
//   --pose file   a file Rich attached, as the phone fetches it: one message, the reply with the lab's
//                 attached files (`lab mac` with RICHOS_MOBILE_LAB_ATTACH), then each card opened
//                 through the core (`openRichFile`) into a folder under the lab's cache, each fetch's
//                 end state and reason printed with every request it made. Prints
//                 {"pose":"file","files":[{"name":..,"state":..,"reason":..,"bytes":..}]}; exit 0 when
//                 every file came over, 1 when any did not, 2 when it could not run.
//
// Prints one line per event on the UTC clock (the core's `send:` account, marks, launches, the stream's
// open, answer and first bytes, and every connection action the core receives), then one JSON line:
// {"tries":N,"stayed":K,"perTry":[...],"replyAfterReturnSeconds":[...]}. Exit 0 when every message left
// while hidden, 1 when any stayed until the return, 2 when it could not run (lab refused, pairing
// failed). `replyAfterReturnSeconds` is how long after each return the Mac's reply was in the core's
// state (what the screen draws), -1 when it never came inside 30 s.
//
// Storage is in memory (`MemoryStorage`): every run starts with a fresh background allowance, so tries
// are never limited by the six-an-hour lease the phone keeps.

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(("lab-phone: " + message + "\n").data(using: .utf8)!)
    exit(2)
}

var options: [String: String] = [:]
var argv = CommandLine.arguments.dropFirst()
while let flag = argv.first {
    argv = argv.dropFirst()
    guard flag.hasPrefix("--"), let value = argv.first else { fail("expected --flag value pairs; see the header of main.swift") }
    argv = argv.dropFirst()
    options[String(flag.dropFirst(2))] = value
}
guard let labPath = options["lab"], let marksPath = options["marks"] else { fail("--lab and --marks are required") }
let tries = Int(options["tries"] ?? "3") ?? 0
let mode = options["mode"] ?? "fresh"
let homeAfter = Double(options["home-after"] ?? "2.0") ?? -1
let hiddenFor = Double(options["hidden"] ?? "25") ?? -1
let pose = options["pose"] ?? "send"
let offlineAt = Double(options["offline-at"] ?? "3.25") ?? -1
let offlineFor = Double(options["offline-for"] ?? "3.4") ?? -1
let awayBefore = Double(options["away-before"] ?? "0") ?? -1
guard awayBefore >= 0 else { fail("bad --away-before") }
guard tries >= 1, ["fresh", "return"].contains(mode), homeAfter >= 0, hiddenFor >= 1 else { fail("bad --tries, --mode, --home-after or --hidden") }
guard ["send", "wifi-drop", "file"].contains(pose), offlineAt >= 0, offlineFor > 0, offlineAt + offlineFor < hiddenFor else {
    fail("bad --pose, --offline-at or --offline-for (the network must be back before the return)")
}

struct LabState: Decodable { var pairLink: String?; var data: String; var `protocol`: String?; var port: Int? }
guard let labData = FileManager.default.contents(atPath: labPath + "/mac.json"),
      let lab = try? JSONDecoder().decode(LabState.self, from: labData) else { fail("cannot read \(labPath)/mac.json") }
guard let owner = FileManager.default.contents(atPath: lab.data + "/lab-owner"),
      String(decoding: owner, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines) == "richos-mobile-isolated-v1" else {
    fail("\(lab.data) has no isolated lab owner marker: refusing to pair with it")
}
guard let link = lab.pairLink else { fail("the lab has no open pairing link (already paired? start a fresh lab)") }
guard FileManager.default.fileExists(atPath: marksPath) || FileManager.default.createFile(atPath: marksPath, contents: nil) else {
    fail("cannot write \(marksPath)")
}
/// The lab's loopback proxy when it has no managed route; `nil`: the paired origin itself.
let routedOrigin: String? = {
    guard lab.protocol == "https" else { return nil }
    guard let port = lab.port else { fail("\(labPath)/mac.json names no proxy port") }
    return "http://127.0.0.1:\(port)"
}()

@Sendable func say(_ line: String) {
    let f = DateFormatter(); f.locale = Locale(identifier: "en_US_POSIX"); f.dateFormat = "HH:mm:ss.SSS"
    f.timeZone = TimeZone(identifier: "UTC")
    print("\(f.string(from: Date()))Z \(line)"); fflush(stdout)
}
func mark(_ label: String) {
    let line = "PHONE_STEP {\"do\":\"mark\",\"detail\":{\"label\":\"\(label)\"},\"end\":\(Date().timeIntervalSince1970)}\n"
    guard let handle = FileHandle(forWritingAtPath: marksPath) else { fail("cannot append to \(marksPath)") }
    handle.seekToEndOfFile(); handle.write(line.data(using: .utf8)!); try? handle.close()
    say("mark: \(label)")
}
func nowMs() -> Int64 { SystemClock().nowMs() }
func pause(_ seconds: Double) async { try? await Task.sleep(nanoseconds: UInt64(seconds * 1e9)) }

/// The production transport, unchanged, with three things around it for the lab: the origin it is
/// handed (the lab's loopback proxy when there is no managed route), a network switch for the wifi-drop
/// pose (off: a request fails at once, URLSession's own `notConnectedToInternet`), and a line for the
/// stream's open, its answer and its first bytes, which is what tells "the Mac answered late" from
/// "the answer arrived and the phone sat on it".
final class LabRoute: HTTPTransport, EventStreamTransport, @unchecked Sendable {
    private let real = URLSessionTransport()
    private let lock = NSLock()
    private var online = true
    private var streams = 0

    var networkOn: Bool {
        get { lock.lock(); defer { lock.unlock() }; return online }
        set { lock.lock(); online = newValue; lock.unlock(); say("network: \(newValue ? "on" : "off")") }
    }

    private func origin(_ paired: String) -> String { routedOrigin ?? paired }

    private static func path(_ target: String) -> String { String(target.split(separator: "?").first ?? "") }

    func send(_ request: HTTPRequest, origin: String) async throws -> HTTPResponse {
        guard networkOn else { throw URLError(.notConnectedToInternet) }
        let started = Date()
        do {
            let answer = try await real.send(request, origin: self.origin(origin))
            say("http: \(request.method) \(Self.path(request.target)) \(answer.status) after \(Int(Date().timeIntervalSince(started) * 1000)) ms")
            return answer
        } catch {
            say("http: \(request.method) \(Self.path(request.target)) failed after \(Int(Date().timeIntervalSince(started) * 1000)) ms")
            throw error
        }
    }

    func open(_ request: HTTPRequest, origin: String) async throws -> (response: HTTPResponse, bytes: AsyncThrowingStream<Data, Error>) {
        guard networkOn else { throw URLError(.notConnectedToInternet) }
        let n: Int = { lock.lock(); defer { lock.unlock() }; streams += 1; return streams }()
        let since = request.target.components(separatedBy: "since=").dropFirst().first?.prefix { $0.isNumber } ?? "none"
        let started = Date()
        say("stream \(n): open, since \(since)")
        let (head, bytes) = try await real.open(request, origin: self.origin(origin))
        say("stream \(n): answered \(head.status) after \(Int(Date().timeIntervalSince(started) * 1000)) ms")
        let logged = AsyncThrowingStream<Data, Error> { continuation in
            let task = Task {
                var lines = 0
                do {
                    for try await line in bytes {
                        lines += 1
                        let text = String(decoding: line, as: UTF8.self).trimmingCharacters(in: .newlines)
                        // Field names only, never a message: `event:`, `id:` and comments are logged whole,
                        // a `data:` line by its length.
                        if text.hasPrefix("event:") || text.hasPrefix("id:") || text.hasPrefix(":") {
                            say("stream \(n): +\(Int(Date().timeIntervalSince(started) * 1000)) ms \(text)")
                        } else if text.hasPrefix("data:") {
                            say("stream \(n): +\(Int(Date().timeIntervalSince(started) * 1000)) ms data (\(line.count) bytes)")
                        }
                        continuation.yield(line)
                    }
                    say("stream \(n): ended by the Mac after \(lines) lines")
                    continuation.finish()
                } catch {
                    say("stream \(n): closed (\(type(of: error)))")
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        return (head, logged)
    }
}

/// Stands in for UIKit's background time: records the core asking for it and giving it back.
@MainActor final class LoggedBackgroundTime: BackgroundContinuation {
    func begin(expired: @escaping @MainActor @Sendable () -> Void) { say("os: background time asked for") }
    func end() { say("os: background time given back") }
}

@MainActor func until(_ what: String, _ seconds: Double, _ condition: @MainActor () -> Bool) async -> Bool {
    let end = Date().addingTimeInterval(seconds)
    while Date() < end { if condition() { return true }; await pause(0.05) }
    say("timed out waiting for \(what)")
    return false
}

/// One word for each connection action the core is told, so the run says when the stream opened, was
/// lost, and when a reply reached the state.
@Sendable func describe(_ action: Action) -> String? {
    switch action {
    case .connected: return "connected"
    case .connectionLost: return "connection lost"
    case .messagesArrived(let messages): return "messages arrived: \(messages.count)"
    case .replyStarted: return "reply started"
    case .replyFinished: return "reply finished"
    case .pairingRevoked: return "pairing revoked"
    default: return nil
    }
}

let storage = MemoryStorage()
let identities = MemoryIdentityStore()
let route = LabRoute()
if let routedOrigin { say("route: the lab's loopback proxy \(routedOrigin) (no managed route)") }

/// A launch: a new store, network handler and URLSession over the saved state and keys.
@MainActor func launch(_ why: String) async -> AppStore {
    let network = NetworkEffects(transport: route, stream: route, identities: identities,
                                 richFiles: URL(fileURLWithPath: labPath).appendingPathComponent("lab-phone-files", isDirectory: true))
    let store = await AppStore.launch(storage: storage, effects: network)
    store.sendLog = { say("send: " + $0) }
    store.backgroundContinuation = LoggedBackgroundTime()
    await network.setSink { action in
        if let line = describe(action) { say("core: " + line) }
        await MainActor.run { store.receive(action) }
    }
    say("launched (\(why))")
    return store
}

/// The app's clock while on screen (`TickSchedule` in the app): pairing's asks and retries need it.
@MainActor func clockTicks(_ store: AppStore, _ onScreen: @escaping @MainActor () -> Bool) -> Task<Void, Never> {
    Task { @MainActor in
        while !Task.isCancelled {
            if onScreen(), store.ticking { store.send(.tick(at: nowMs())) }
            await pause(0.25)
        }
    }
}

@MainActor func run() async -> Int32 {
    var store = await launch("first")
    var onScreen = true
    var ticks = clockTicks(store) { onScreen }
    store.becameActive(at: nowMs())
    store.send(.submitPairingLink(text: link))
    guard await until("the six words", 30, { store.state.pairing == .confirming }) else { return 2 }
    store.send(.confirmWords)
    guard await until("the Mac's press", 40, { store.state.pairing == .paired }) else { return 2 }
    if !store.state.consentGiven { store.send(.acceptConsent) }
    guard await until("the stream", 20, { store.state.linkOpen }) else { return 2 }
    say("paired with the isolated lab")
    if pose == "file" { return await files(store) }

    var perTry: [String] = []
    var replyAfterReturn: [Double] = []
    for n in 1...tries {
        if mode == "fresh" {
            onScreen = false
            store.wentToBackground(at: nowMs())
            await store.settle()
            ticks.cancel()
            store = await launch("fresh, try \(n)")
            onScreen = true
            ticks = clockTicks(store) { onScreen }
            store.becameActive(at: nowMs())
            _ = await until("the stream", 20, { store.state.linkOpen })
            await pause(4)
        } else {
            await pause(3)
        }
        if awayBefore > 0 {
            // The walk's runner left the app between tries (to read the Wi-Fi switch in Settings) and
            // came back about 5.5 s before the next "PAUSE MAC": the stream closes and opens again.
            onScreen = false
            store.wentToBackground(at: nowMs())
            mark("away before \(n)")
            await pause(awayBefore)
            onScreen = true
            store.becameActive(at: nowMs())
            mark("back before \(n)")
            await pause(5.5)
        }
        let text = (pose == "wifi-drop" ? "hang " : "in flight ") + "\(n)"
        mark("PAUSE MAC \(n)")
        await pause(1)
        store.send(.compose(text: text))
        store.send(.sendDraft(clientID: UUID().uuidString.lowercased(), at: nowMs()))
        await pause(homeAfter)
        onScreen = false
        store.wentToBackground(at: nowMs())
        let home = Date()
        await pause(0.5)
        mark("hidden \(n)")
        var leftWhileHidden = false
        var dropped = false, restored = false
        let back = home.addingTimeInterval(hiddenFor)
        while Date() < back {
            if store.state.outbox.isEmpty { leftWhileHidden = true }
            if pose == "wifi-drop", !dropped, Date() >= home.addingTimeInterval(offlineAt) {
                dropped = true
                route.networkOn = false
                mark("RELEASE MAC \(n)")
            }
            if pose == "wifi-drop", dropped, !restored, Date() >= home.addingTimeInterval(offlineAt + offlineFor) {
                restored = true
                route.networkOn = true
            }
            await pause(0.05)
        }
        mark("returning \(n)")
        onScreen = true
        let returned = Date()
        store.becameActive(at: nowMs())
        perTry.append(leftWhileHidden ? "left while hidden" : "stayed until the return")
        say("try \(n): \(perTry.last!)")
        let arrived = await until("the reply to try \(n)", 30, {
            store.state.messages.contains { $0.author == .rich && $0.text.contains("ack: \(text)") }
        })
        let seconds = arrived ? (Date().timeIntervalSince(returned) * 100).rounded() / 100 : -1
        replyAfterReturn.append(seconds)
        say("try \(n): reply in the state \(arrived ? "\(seconds) s" : "never (30 s)") after the return")
    }
    onScreen = false
    store.wentToBackground(at: nowMs())
    await store.settle()
    ticks.cancel()
    let stayed = perTry.filter { $0 != "left while hidden" }.count
    let result: [String: Any] = ["tries": tries, "stayed": stayed, "perTry": perTry, "mode": mode, "pose": pose,
                                 "replyAfterReturnSeconds": replyAfterReturn]
    print(String(decoding: try! JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]), as: UTF8.self))
    return stayed == 0 ? 0 : 1
}

/// `--pose file`: one message, the reply's cards, each opened as a tap opens it.
@MainActor func files(_ store: AppStore) async -> Int32 {
    store.send(.compose(text: "send me the files"))
    store.send(.sendDraft(clientID: UUID().uuidString.lowercased(), at: nowMs()))
    guard await until("the reply's files", 40, { store.state.messages.contains { $0.richFile != nil } }) else { return 2 }
    await pause(2)
    var out: [[String: Any]] = []
    for file in store.state.messages.compactMap(\.richFile) {
        say("file \(file.id): open")
        store.send(.openRichFile(id: file.id))
        _ = await until("file \(file.id) to settle", 40, {
            let state = store.state.fileFetches[file.id]?.state
            return state == .kept || state == .failed
        })
        let fetch = store.state.fileFetches[file.id]
        let size = (fetch?.path).flatMap { try? FileManager.default.attributesOfItem(atPath: $0)[.size] as? Int } ?? -1
        say("file \(file.id): \(fetch?.state.rawValue ?? "none")\(fetch?.reason.map { " (\($0))" } ?? "")")
        out.append(["name": file.name, "state": fetch?.state.rawValue ?? "none", "reason": fetch?.reason ?? "", "bytes": size])
        store.send(.richFileViewed(id: file.id))
    }
    print(String(decoding: try! JSONSerialization.data(withJSONObject: ["pose": "file", "files": out], options: [.sortedKeys]),
                 as: UTF8.self))
    return out.allSatisfy { ($0["state"] as? String) == "kept" } ? 0 : 1
}

Task { @MainActor in exit(await run()) }
RunLoop.main.run()
