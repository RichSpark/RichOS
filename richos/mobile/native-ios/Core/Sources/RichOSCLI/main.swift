// `rios-cli` — the executable behind `bin/rios`. Run it through `bin/rios`, which builds it
// incrementally and supplies the external cache directory.
//
// Grammar and output are the preserved mobile CLI's (`richos/mobile/cli/mobile.mjs`; build plan
// §3.1 "Grammar"): JSON on stdout `{ok, mode, command, elapsedMs, result}`; on failure JSON on
// stderr `{ok:false, error, elapsedMs}` and exit 1.
import Foundation
#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif
import RichOSCore
import RichOSFixtures
import RichOSPerfSeed

let usage = """
RichOS native iPhone development loop (JSON output; nonzero exit on failure)
  bin/rios headless state|reset|restart
  bin/rios headless fixture \(Fixture.all.map(\.name).joined(separator: "|"))
  bin/rios headless action '{"type":"compose","text":"Hello"}'
  bin/rios headless action '{"type":"send"}'                  (stamps clientId and at; pass them to replay)
  bin/rios headless action '{"type":"network","online":false}'
  bin/rios headless action '{"type":"set-appearance","appearance":"light"}'
  Action names are the preserved mobile CLI's wherever it already names the action
  (send, network, retry, discard, pair, confirm-pair, forget-pair, older, ...); every name is
  listed in the error for an unknown one.
  bin/rios headless scenario \(Scenario.all.map(\.name).joined(separator: "|"))
  bin/rios sim prepare [fixture]      generate, build, create+boot the simulator, install, launch
  bin/rios sim launch [fixture]       relaunch the app process (optionally straight into a fixture)
  bin/rios sim restart                relaunch the app process
  bin/rios sim verify                 headless and simulator results must be byte-identical
  bin/rios sim screenshot [path]      capture the screen; default name <screen>-<appearance>.png
  bin/rios sim ui-test [TARGET/SUITE] the UI and unit tests on this simulator, or only one target,
                                      suite or test (xcodebuild -only-testing)
  bin/rios sim check-release          prove fixtures and the bridge are absent from Release
  bin/rios sim build Debug|Release    build only
  bin/rios sim stop                   terminate the app, shut the simulator down and delete it
  bin/rios sim doctor                 tool versions and the cache path
  bin/rios sim state|reset|fixture|action|scenario   the headless commands, inside the Debug app
  bin/rios test [swift test arguments]      the core's unit tests on this Mac, no simulator
  bin/rios perf-seed FIXTURE_DIR SHA256 OUT_DIR   Android's start-time fixture (perf/condition.py) as
                                      this app's saved state, written through the core and read back;
                                      `perf.py ios` copies OUT_DIR into the installed app (perf/README.md)
  bin/rios screen-text PNG            the text on a screenshot (Vision), one line per entry
Headless commands share one session under the external cache. `sim` uses only the simulator this
command created (recorded in the cache); it never addresses "booted".
RICHOS_NATIVE_IOS_CACHE overrides the per-checkout cache directory.
"""

struct Output<Result: Encodable>: Encodable {
    var ok = true
    var mode: String
    var command: String
    var elapsedMs: Double
    var result: Result
}

struct Failure: Encodable {
    var ok = false
    var error: String
    var elapsedMs: Double
}

let started = DispatchTime.now().uptimeNanoseconds
func elapsedMs() -> Double {
    (Double(DispatchTime.now().uptimeNanoseconds - started) / 1_000_000 * 100).rounded() / 100
}

func emit<T: Encodable>(_ value: T, to handle: FileHandle) {
    let data = (try? CoreJSON.encode(value, pretty: true)) ?? Data("{}".utf8)
    handle.write(data)
    handle.write(Data("\n".utf8))
}

/// Parses `<command> [arg]` into the shared envelope, exactly as `mobile.mjs` `payload()` does.
func payload(_ command: String, _ arg: String?) throws -> Command {
    guard let kind = Command.Kind(rawValue: command) else {
        throw CoreError("Unknown command: \(command). Use --help.")
    }
    switch kind {
    case .state, .reset, .restart:
        return Command(kind)
    case .fixture, .scenario:
        guard let arg else { throw CoreError("\(command) needs a name") }
        return Command(kind, name: arg)
    case .action:
        guard let arg else { throw CoreError("action needs a JSON object") }
        do {
            return Command(.action, action: try CoreJSON.decode(Action.self, from: Data(arg.utf8)))
        } catch let DecodingError.dataCorrupted(context) {
            throw CoreError("invalid action: \(context.debugDescription)")
        } catch DecodingError.keyNotFound(let key, _) {
            throw CoreError("invalid action: missing '\(key.stringValue)'")
        } catch {
            throw CoreError("invalid action: \(error)")
        }
    }
}

func cacheDirectory() throws -> URL {
    guard let path = ProcessInfo.processInfo.environment["RICHOS_NATIVE_IOS_CACHE"], !path.isEmpty else {
        throw CoreError("RICHOS_NATIVE_IOS_CACHE is not set; run through bin/rios")
    }
    guard path.hasPrefix("/") else { throw CoreError("the cache needs an absolute path") }
    let url = URL(fileURLWithPath: path).standardizedFileURL.resolvingSymlinksInPath()
    let parts = url.pathComponents
    if parts.count >= 3 && parts[1] == "Volumes" {
        let volume = "/Volumes/" + parts[2]
        var mounted = stat(), parent = stat()
        guard stat(volume, &mounted) == 0 && stat("/Volumes", &parent) == 0 && mounted.st_dev != parent.st_dev else {
            throw CoreError("the selected cache volume is not mounted: \(volume)")
        }
    }
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

/// A persistent kernel lock is released even after SIGKILL. Never unlink its inode.
/// Legacy directory locks are removed only when their recorded owner is provably gone.
func withLock<T>(_ cache: URL, _ name: String, _ body: () async throws -> T) async throws -> T {
    let lock = cache.appendingPathComponent(name + ".flock")
    let fd = open(lock.path, O_CREAT | O_RDWR | O_CLOEXEC, mode_t(0o600))
    guard fd >= 0 else { throw CoreError("Cannot open command lock \(lock.path): errno \(errno)") }
    defer { close(fd) }
    guard flock(fd, LOCK_EX | LOCK_NB) == 0 else {
        throw CoreError("Another rios command owns \(lock.path)")
    }
    defer { flock(fd, LOCK_UN) }
    let legacy = cache.appendingPathComponent(name)
    if FileManager.default.fileExists(atPath: legacy.path) {
        guard let data = try? Data(contentsOf: legacy.appendingPathComponent("owner.json")),
              let owner = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let number = owner["pid"] as? NSNumber,
              let pid = Int32(exactly: number.int64Value), pid > 0, number.doubleValue == Double(pid),
              kill(pid, 0) == -1, errno == ESRCH else {
            throw CoreError("Legacy command lock has a live or unverified owner: \(legacy.path)")
        }
        try FileManager.default.removeItem(at: legacy)
    }
    return try await body()
}

func run() async -> Int32 {
    let args = Array(CommandLine.arguments.dropFirst())
    guard let mode = args.first, mode != "--help", mode != "-h" else {
        print(usage)
        return 0
    }
    let command = args.count > 1 ? args[1] : mode
    do {
        switch mode {
        case "headless":
            let cache = try cacheDirectory()
            let result = try await withLock(cache, "headless.lock") {
                let host = try await HeadlessHost(storage: FileStorage(directory: cache.appendingPathComponent("headless")))
                return try await CommandRunner.execute(try payload(command, args.count > 2 ? args[2] : nil), on: host)
            }
            emit(Output(mode: mode, command: command, elapsedMs: elapsedMs(), result: result), to: .standardOutput)
        case "perf-seed":
            guard args.count == 4 else { throw CoreError("perf-seed FIXTURE_DIR SHA256 OUT_DIR") }
            let report = try await PerfSeed.write(fixtureDirectory: URL(fileURLWithPath: args[1]), expectedSHA256: args[2],
                                                  into: URL(fileURLWithPath: args[3]))
            emit(Output(mode: mode, command: mode, elapsedMs: elapsedMs(), result: report), to: .standardOutput)
        case "screen-text":
            #if os(macOS)
            guard args.count == 2 else { throw CoreError("screen-text PNG") }
            let lines = try ScreenText.read(URL(fileURLWithPath: args[1]))
            emit(Output(mode: mode, command: mode, elapsedMs: elapsedMs(), result: ["lines": lines]), to: .standardOutput)
            #else
            throw CoreError("screen-text runs on macOS only")
            #endif
        case "sim":
            #if os(macOS)
            let cache = try cacheDirectory()
            let report = try await withLock(cache, "sim.lock") {
                let simulator = try Simulator(cache: cache)
                return try await simulator.run(command, args.count > 2 ? args[2] : nil)
            }
            emit(Output(mode: mode, command: command, elapsedMs: elapsedMs(), result: report), to: .standardOutput)
            #else
            throw CoreError("sim mode runs on macOS only")
            #endif
        default:
            throw CoreError("Unknown mode: \(mode). Use --help.")
        }
        return 0
    } catch {
        emit(Failure(error: String(describing: error), elapsedMs: elapsedMs()), to: .standardError)
        return 1
    }
}

exit(await run())
