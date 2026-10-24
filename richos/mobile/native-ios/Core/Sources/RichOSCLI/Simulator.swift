// `bin/rios sim …` — the same commands as headless mode, run inside the Debug app on a simulator
// this command created (loop L2, build plan §3.1). macOS only.
//
// Rules it keeps, each for a reason that has already cost this project:
//   * It addresses ONLY the simulator it created, by the UDID it recorded in the cache. It never
//     uses `booted`, which resolves to whichever device happens to be running — another agent's.
//   * It boots with `simctl boot`, which starts no Simulator window; nothing appears on the Mac's
//     screen (ceo-decisions §65).
//   * `sim stop` releases the exclusive lease and shuts it down, retaining the prepared OS.
//   * Device data uses Apple's default simulator storage, the exception the owner approved for the
//     preserved app when an external device set failed with EPERM (richos/mobile/DEVELOPMENT.md,
//     "Registered repository proofs"); build output stays in the external cache.
#if os(macOS)
import Foundation
import RichOSCore
import RichOSFixtures
import RichOSPerfSeed

struct SimReport: Encodable {
    var state: AppState?
    var name: String?
    var trace: [TraceStep]?
    var device: String?
    var app: String?
    var log: String?
    var screenshot: String?
    var scenarios: [ScenarioParity]?
    var processRestartPreservedState: Bool?
    var tests: TestCounts?
    var resultBundle: String?
    var releaseApp: String?
    var markersAbsentFromRelease: [String]?
    var markersPresentInDebug: [String]?
    var seedingMarkersAbsentFromDebug: [String]?
    var tools: [String: String]?
    var deleted: String?
    var retained: String?
    var note: String?
    /// Wall milliseconds per step, so every run reports its own L2 cost.
    var timingsMs: [String: Double]?
}

struct ScenarioParity: Encodable {
    var name: String
    var steps: Int
    var identical: Bool
}

struct TestCounts: Codable {
    var total: Int
    var passed: Int
    var failed: Int
    var skipped: Int
}

final class Simulator {
    static let bundleID = "dev.richos.connect"
    static let scheme = "RichOSNative"
    /// iPhone 16 Pro is 402 × 874 points — round 12's "current" device, so a screenshot is directly
    /// comparable with the mockup at the same size.
    static let deviceTypeName = "iPhone 16 Pro"
    /// Strings that exist only in development code. The Release binary must contain none of them,
    /// and the Debug binary must contain all of them (a negative check needs its positive probe).
    static let developmentMarkers = ["rios-commands", "rios-fixture", "rios-interactive-fixture", "rios-appearance", "rios-notifications", "rios-cards", "compose-draft", "Henderson proposal"]
    /// The start-time measurement's seeder (`RichOSPerfSeed`) writes the app's saved state from the
    /// Mac; no app build may contain it, Debug included. Its positive probe is this CLI's own binary.
    static let seedingMarkers = [PerfSeed.marker]

    let cache: URL
    let root: URL
    private var timings: [String: Double] = [:]

    init(cache: URL) throws {
        self.cache = cache
        guard let rootPath = ProcessInfo.processInfo.environment["RICHOS_NATIVE_IOS_ROOT"] else {
            throw CoreError("RICHOS_NATIVE_IOS_ROOT is not set; run through bin/rios")
        }
        root = URL(fileURLWithPath: rootPath)
    }

    // MARK: - dispatch

    func run(_ command: String, _ arg: String?) async throws -> SimReport {
        var report: SimReport
        if !["prepare", "ui-test", "stop", "doctor", "build", "check-release"].contains(command), let udid = recordedDevice(),
           try allDevices().contains(where: { $0["udid"] as? String == udid }) {
            try registerOwner(udid)
        }
        switch command {
        case "prepare": report = try await prepare(fixture: arg)
        case "launch": report = try await launch(fixture: arg)
        case "restart": report = try await launch(fixture: nil)
        case "verify": report = try await verify()
        case "ui-test": report = try uiTest(only: arg)
        case "screenshot": report = try await screenshot(arg)
        case "check-release": report = try checkRelease()
        case "stop": report = try stop()
        case "doctor": report = try doctor()
        case "build":
            let app = try build(configuration: arg ?? "Debug")
            report = SimReport(app: app.path, log: log("build-\(arg ?? "Debug")").path)
        default:
            let result = try await request(try payload(command, arg))
            report = SimReport(state: result.state, name: result.name, trace: result.trace)
        }
        report.device = report.device ?? recordedDevice()
        report.timingsMs = timings.isEmpty ? nil : timings
        return report
    }

    // MARK: - build

    private func log(_ name: String) -> URL { cache.appendingPathComponent("logs/\(name).log") }
    private var projectPath: URL { cache.appendingPathComponent("project/RichOSNative.xcodeproj") }
    private var derivedData: URL { cache.appendingPathComponent("DerivedData") }

    /// Generates the Xcode project into the cache. `--use-cache` makes an unchanged spec a no-op;
    /// source files are synchronized folders, so adding one needs no regeneration at all.
    func generateProject() throws {
        // XcodeGen writes to a temporary file and moves it into place; the directory must exist.
        try FileManager.default.createDirectory(at: cache.appendingPathComponent("project"), withIntermediateDirectories: true)
        try timed("generate") {
            try Tool.run("xcodegen", ["generate", "--spec", root.appendingPathComponent("project.yml").path,
                                      "--project", cache.appendingPathComponent("project").path,
                                      "--use-cache", "--cache-path", cache.appendingPathComponent("xcodegen.cache").path,
                                      "--quiet"], log: log("xcodegen"))
        }
    }

    func build(configuration: String) throws -> URL {
        guard ["Debug", "Release"].contains(configuration) else { throw CoreError("configuration must be Debug or Release") }
        try generateProject()
        try timed("build-\(configuration)") {
            try Tool.run("xcodebuild", ["-project", projectPath.path, "-scheme", Self.scheme,
                                        "-configuration", configuration, "-sdk", "iphonesimulator",
                                        "-destination", "generic/platform=iOS Simulator",
                                        "-derivedDataPath", derivedData.path,
                                        "-clonedSourcePackagesDirPath", cache.appendingPathComponent("SourcePackages").path,
                                        "CODE_SIGN_IDENTITY=-", "build"], log: log("build-\(configuration)"))
        }
        return derivedData.appendingPathComponent("Build/Products/\(configuration)-iphonesimulator/RichOSNative.app")
    }

    // MARK: - the device this command created

    private var deviceRecord: URL { cache.appendingPathComponent("simulator.json") }

    func recordedDevice() -> String? {
        guard let data = try? Data(contentsOf: deviceRecord),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: String] else { return nil }
        return object["udid"]
    }

    private func allDevices() throws -> [[String: Any]] {
        let out = try Tool.run("xcrun", ["simctl", "list", "devices", "--json"]).stdout
        let object = try JSONSerialization.jsonObject(with: Data(out.utf8)) as? [String: Any]
        let groups = object?["devices"] as? [String: [[String: Any]]] ?? [:]
        return groups.values.flatMap { $0 }
    }

    /// Lease the shared prepared simulator for this device type and runtime.
    func device() throws -> String {
        let runtimesJSON = try Tool.run("xcrun", ["simctl", "list", "runtimes", "--json"]).stdout
        let runtimes = ((try JSONSerialization.jsonObject(with: Data(runtimesJSON.utf8)) as? [String: Any])?["runtimes"] as? [[String: Any]] ?? [])
            .filter { ($0["isAvailable"] as? Bool) == true && (($0["name"] as? String)?.hasPrefix("iOS") ?? false) }
        guard let runtime = runtimes.last?["identifier"] as? String else {
            throw CoreError("no available iOS simulator runtime; install one in Xcode")
        }
        let typesJSON = try Tool.run("xcrun", ["simctl", "list", "devicetypes", "--json"]).stdout
        let types = (try JSONSerialization.jsonObject(with: Data(typesJSON.utf8)) as? [String: Any])?["devicetypes"] as? [[String: Any]] ?? []
        guard let type = types.first(where: { $0["name"] as? String == Self.deviceTypeName })?["identifier"] as? String else {
            throw CoreError("device type '\(Self.deviceTypeName)' is not installed")
        }
        let collector = root.appendingPathComponent("../../engine/scripts/lib/testdevices.py").standardizedFileURL
        let checkout = root.appendingPathComponent("../../..").standardizedFileURL
        let udid = try Tool.run("python3", [collector.path, "acquire-ios", "--type", type, "--runtime", runtime,
                                          "--checkout", checkout.path]).stdout
            .trimmingCharacters(in: .whitespacesAndNewlines)
        try FileManager.default.createDirectory(at: cache, withIntermediateDirectories: true)
        try CoreJSON.encode(["udid": udid, "runtime": runtime, "type": type]).write(to: deviceRecord, options: .atomic)
        return udid
    }

    private func registerOwner(_ udid: String) throws {
        let collector = root.appendingPathComponent("../../engine/scripts/lib/testdevices.py").standardizedFileURL
        let checkout = root.appendingPathComponent("../../..").standardizedFileURL
        try Tool.run("python3", [collector.path, "use-ios", "--id", udid, "--checkout", checkout.path])
    }

    func boot() throws -> String {
        let udid = try device()
        try registerOwner(udid)
        let state = try allDevices().first(where: { $0["udid"] as? String == udid })?["state"] as? String
        try timed("boot") {
            if state != "Booted" {
                let admission = root.appendingPathComponent("../../engine/scripts/lib/testdevices.py").standardizedFileURL
                try Tool.run("python3", [admission.path, "boot-ios", "--id", udid])
            }
            try Tool.run("xcrun", ["simctl", "bootstatus", udid, "-b"])
        }
        return udid
    }

    // MARK: - the mailbox

    private func container(_ udid: String) throws -> URL {
        let collector = root.appendingPathComponent("../../engine/scripts/lib/testdevices.py").standardizedFileURL
        try Tool.run("python3", [collector.path, "touch-lease", "--kind", "ios-simulator", "--id", udid])
        let path = try Tool.run("xcrun", ["simctl", "get_app_container", udid, Self.bundleID, "data"]).stdout
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return URL(fileURLWithPath: path)
    }

    func request(_ command: Command, timeout: TimeInterval = 15) async throws -> CommandResult {
        guard let udid = recordedDevice() else { throw CoreError("no simulator yet; run `bin/rios sim prepare`") }
        let mailbox = try container(udid).appendingPathComponent("Documents/\(DevBridgeNames.directory)")
        try FileManager.default.createDirectory(at: mailbox, withIntermediateDirectories: true)
        let token = UUID().uuidString
        let request = mailbox.appendingPathComponent("\(token).request.json")
        let response = mailbox.appendingPathComponent("\(token).response.json")
        let staging = mailbox.appendingPathComponent("\(token).request.json.new")
        try CoreJSON.encode(command).write(to: staging, options: .atomic)
        try FileManager.default.moveItem(at: staging, to: request)
        let started = Date()
        defer {
            try? FileManager.default.removeItem(at: request)
            try? FileManager.default.removeItem(at: response)
        }
        while !FileManager.default.fileExists(atPath: response.path) {
            if Date().timeIntervalSince(started) > timeout {
                throw CoreError("the Debug app did not answer within \(Int(timeout)) s; is it running? (`bin/rios sim prepare`)")
            }
            try await Task.sleep(for: .milliseconds(20))
        }
        timings["request-\(command.command.rawValue)", default: 0] += Date().timeIntervalSince(started) * 1000
        let reply = try CoreJSON.decode(CommandResponse.self, from: Data(contentsOf: response))
        guard reply.ok, let result = reply.result else { throw CoreError(reply.error ?? "the app refused the command") }
        return result
    }

    // MARK: - commands

    func prepare(fixture: String?) async throws -> SimReport {
        let app = try build(configuration: "Debug")
        let udid = try boot()
        try timed("install") { try Tool.run("xcrun", ["simctl", "install", udid, app.path]) }
        var report = try await launch(fixture: fixture)
        report.app = app.path
        return report
    }

    /// A real process start (the preserved CLI's `sim restart`): terminate, launch, wait until the
    /// development bridge answers, and return its state.
    func launch(fixture: String?) async throws -> SimReport {
        guard let udid = recordedDevice() else { throw CoreError("no simulator yet; run `bin/rios sim prepare`") }
        var args = ["simctl", "launch", "--terminate-running-process", udid, Self.bundleID]
        if let fixture {
            _ = try Fixture.named(fixture)  // refuse an unknown name before launching anything
            args += ["-\(DevBridgeNames.fixtureArgument)", fixture]
        }
        let started = Date()
        try Tool.run("xcrun", args)
        // A first launch on a busy Mac can be slow; the bridge answers as soon as the app is up.
        let result = try await request(Command(.state), timeout: 60)
        timings["launch-to-first-state"] = Date().timeIntervalSince(started) * 1000
        return SimReport(state: result.state)
    }

    /// Headless and native must produce byte-identical results for every scenario, and a queued
    /// draft must survive a real process termination.
    func verify() async throws -> SimReport {
        var parity: [ScenarioParity] = []
        for scenario in Scenario.all {
            let host = try await HeadlessHost(storage: MemoryStorage())
            let headless = try CoreJSON.encode(try await CommandRunner.execute(Command(.scenario, name: scenario.name), on: host))
            let native = try CoreJSON.encode(try await request(Command(.scenario, name: scenario.name)))
            guard headless == native else {
                throw CoreError("\(scenario.name): simulator and headless results differ\nheadless: \(String(decoding: headless, as: UTF8.self))\nnative:   \(String(decoding: native, as: UTF8.self))")
            }
            parity.append(ScenarioParity(name: scenario.name, steps: scenario.steps.count, identical: true))
        }
        _ = try await request(Command(.fixture, name: "conv-empty"))
        let before = try await request(Command(.action, action: .compose(text: "Survives process termination")))
        let after = try await launch(fixture: nil)
        // What a relaunch must keep is the durable state; transient fields (a connection attempt the
        // fresh process makes, the cached-history marker) are rebuilt by design. So is the
        // appearance: every launch adopts the phone's own light or dark setting before the first
        // frame (`followPhone(SystemAppearance.current())`, RichOSNativeApp.swift), so a fixture's
        // dark relaunches in the simulator's light and that is correct, not lost state.
        var expected = before.state.persisted
        if let relaunched = after.state?.persisted { expected.appearance = relaunched.appearance }
        guard after.state?.persisted == expected else {
            throw CoreError("durable state changed across a real process restart")
        }
        _ = try await request(Command(.fixture, name: "conv-empty"))
        return SimReport(scenarios: parity, processRestartPreservedState: true)
    }

    func screenshot(_ path: String?) async throws -> SimReport {
        guard let udid = recordedDevice() else { throw CoreError("no simulator yet; run `bin/rios sim prepare`") }
        let state = try await request(Command(.state)).state
        let target = path.map { URL(fileURLWithPath: $0) }
            ?? cache.appendingPathComponent("screenshots/\(state.screen.rawValue)-\(state.appearance.rawValue).png")
        try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
        try timed("screenshot") { try Tool.run("xcrun", ["simctl", "io", udid, "screenshot", target.path]) }
        return SimReport(state: state, screenshot: target.path)
    }

    /// `only` (a test target, suite or test, as xcodebuild's `-only-testing:` takes it) runs that
    /// alone, e.g. `RichOSNativeTests/TranscriptItemIDTests`; nil runs every test.
    func uiTest(only: String? = nil) throws -> SimReport {
        let files = ["UITests", "UnitTests"].flatMap { dir in
            (try? FileManager.default.subpathsOfDirectory(atPath: root.appendingPathComponent(dir).path))?.filter { $0.hasSuffix(".swift") } ?? []
        }
        guard !files.isEmpty else {
            // A green run of nothing is the defect run-tests.sh exists to stop; say it plainly.
            throw CoreError("UITests/ and UnitTests/ have no test files yet; nothing to run")
        }
        let udid = try boot()
        try generateProject()
        let bundle = cache.appendingPathComponent("results/ui-\(Int(Date().timeIntervalSince1970)).xcresult")
        try FileManager.default.createDirectory(at: bundle.deletingLastPathComponent(), withIntermediateDirectories: true)
        try timed("ui-test") {
            try Tool.run("xcodebuild", ["-project", projectPath.path, "-scheme", Self.scheme, "-configuration", "Debug",
                                        "-destination", "platform=iOS Simulator,id=\(udid)",
                                        "-derivedDataPath", derivedData.path,
                                        "-clonedSourcePackagesDirPath", cache.appendingPathComponent("SourcePackages").path,
                                        "-resultBundlePath", bundle.path,
                                        "CODE_SIGN_IDENTITY=-"] + (only.map { ["-only-testing:\($0)"] } ?? []) + ["test"],
                           log: log("ui-test"))
        }
        let summary = try Tool.run("xcrun", ["xcresulttool", "get", "test-results", "summary", "--path", bundle.path]).stdout
        let object = try JSONSerialization.jsonObject(with: Data(summary.utf8)) as? [String: Any] ?? [:]
        let counts = TestCounts(total: object["totalTestCount"] as? Int ?? 0, passed: object["passedTests"] as? Int ?? 0,
                                failed: object["failedTests"] as? Int ?? 0, skipped: object["skippedTests"] as? Int ?? 0)
        guard counts.total > 0, counts.failed == 0, counts.passed + counts.skipped == counts.total else {
            throw CoreError("UI tests: \(counts.passed) passed, \(counts.failed) failed, \(counts.skipped) skipped of \(counts.total); \(bundle.path)")
        }
        return SimReport(log: log("ui-test").path, tests: counts, resultBundle: bundle.path)
    }

    /// Builds Release and proves the development bridge and fixtures are absent from its binary,
    /// after proving they ARE present in the Debug binary (so the search can find them at all).
    func checkRelease() throws -> SimReport {
        let debug = try build(configuration: "Debug")
        let release = try build(configuration: "Release")
        // Every file in the bundle, not only the main executable: Xcode 16 Debug builds put the code
        // in `RichOSNative.debug.dylib` beside a stub executable (ENABLE_DEBUG_DYLIB), which the
        // positive probe below caught the first time this looked only at the executable.
        func bundleBytes(_ app: URL) throws -> [Data] {
            let files = FileManager.default.enumerator(at: app, includingPropertiesForKeys: [.isRegularFileKey])?
                .compactMap { $0 as? URL }
                .filter { (try? $0.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true } ?? []
            return try files.map { try Data(contentsOf: $0) }
        }
        let debugFiles = try bundleBytes(debug)
        let releaseFiles = try bundleBytes(release)
        var present: [String] = [], leaked: [String] = []
        for marker in Self.developmentMarkers {
            let bytes = Data(marker.utf8)
            if debugFiles.contains(where: { $0.range(of: bytes) != nil }) { present.append(marker) }
            if releaseFiles.contains(where: { $0.range(of: bytes) != nil }) { leaked.append(marker) }
        }
        guard present.count == Self.developmentMarkers.count else {
            let missing = Self.developmentMarkers.filter { !present.contains($0) }
            throw CoreError("the Debug binary lacks \(missing); the Release search would prove nothing")
        }
        guard leaked.isEmpty else { throw CoreError("development code reached the Release binary: \(leaked)") }
        // The seeder: present in rios-cli (this process), absent from BOTH app bundles.
        let cli = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath())
        for marker in Self.seedingMarkers {
            let bytes = Data(marker.utf8)
            guard cli.range(of: bytes) != nil else {
                throw CoreError("rios-cli lacks '\(marker)'; the search of the app bundles would prove nothing")
            }
            if (debugFiles + releaseFiles).contains(where: { $0.range(of: bytes) != nil }) {
                throw CoreError("the start-time seeder ('\(marker)') reached an app bundle")
            }
        }
        return SimReport(app: debug.path, releaseApp: release.path,
                         markersAbsentFromRelease: Self.developmentMarkers + Self.seedingMarkers,
                         markersPresentInDebug: present, seedingMarkersAbsentFromDebug: Self.seedingMarkers)
    }

    /// Ends the lease and shuts down the device, retaining its prepared OS.
    func stop() throws -> SimReport {
        guard let udid = recordedDevice() else { return SimReport(note: "no simulator recorded; nothing to stop") }
        let collector = root.appendingPathComponent("../../engine/scripts/lib/testdevices.py").standardizedFileURL
        let checkout = root.appendingPathComponent("../../..").standardizedFileURL
        try Tool.run("python3", [collector.path, "release-ios", "--id", udid, "--checkout", checkout.path])
        try FileManager.default.removeItem(at: deviceRecord)
        return SimReport(retained: udid, note: "shut down; prepared OS retained for the next run")
    }

    func doctor() throws -> SimReport {
        var tools: [String: String] = [:]
        tools["xcodebuild"] = (try? Tool.run("xcodebuild", ["-version"]).stdout)?.trimmingCharacters(in: .whitespacesAndNewlines)
        tools["xcodegen"] = (try? Tool.run("xcodegen", ["--version"]).stdout)?.trimmingCharacters(in: .whitespacesAndNewlines)
        tools["swift"] = (try? Tool.run("swift", ["--version"]).stdout)?.components(separatedBy: "\n").first
        tools["cache"] = cache.path
        return SimReport(tools: tools)
    }

    private func timed(_ step: String, _ body: () throws -> Void) throws {
        let started = Date()
        try body()
        timings[step, default: 0] += (Date().timeIntervalSince(started) * 1000).rounded()
    }
}

/// The two names the CLI and the Debug app must agree on. Duplicated from `DevBridge.swift` on
/// purpose — the app's copy is compiled out of Release — and checked against it by the app suite.
enum DevBridgeNames {
    static let directory = "rios-commands"
    static let fixtureArgument = "rios-fixture"
    static let appearanceArgument = "rios-appearance"
}

/// Runs a tool to completion. Output goes to a file, never a pipe, so a chatty xcodebuild cannot
/// fill a pipe buffer and deadlock; the file is read back after exit.
enum Tool {
    struct Output { var stdout: String }

    @discardableResult
    static func run(_ tool: String, _ args: [String], log: URL? = nil) throws -> Output {
        let fm = FileManager.default
        let out = log ?? fm.temporaryDirectory.appendingPathComponent("rios-\(UUID().uuidString).out")
        let err = out.appendingPathExtension("stderr")
        try fm.createDirectory(at: out.deletingLastPathComponent(), withIntermediateDirectories: true)
        fm.createFile(atPath: out.path, contents: nil)
        fm.createFile(atPath: err.path, contents: nil)
        defer {
            if log == nil { try? fm.removeItem(at: out) }
            try? fm.removeItem(at: err)
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        if tool == "xcodebuild", !args.contains("-version"),
           let root = ProcessInfo.processInfo.environment["RICHOS_NATIVE_IOS_ROOT"] {
            let runner = URL(fileURLWithPath: root).appendingPathComponent("../../engine/scripts/lib/native-work.py").standardizedFileURL
            process.arguments = ["python3", runner.path, "--", tool] + args
        } else {
            process.arguments = [tool] + args
        }
        process.standardOutput = try FileHandle(forWritingTo: out)
        let errHandle = try FileHandle(forWritingTo: err)
        process.standardError = errHandle
        try process.run()
        process.waitUntilExit()
        let stdout = (try? String(contentsOf: out, encoding: .utf8)) ?? ""
        let stderr = (try? String(contentsOf: err, encoding: .utf8)) ?? ""
        if let log, !stderr.isEmpty, let handle = try? FileHandle(forWritingTo: log) {
            handle.seekToEndOfFile()
            handle.write(Data(stderr.utf8))
            try? handle.close()
        }
        guard process.terminationStatus == 0 else {
            let detail = (stderr.isEmpty ? stdout : stderr).suffix(2000)
            throw CoreError("\(tool) \(args.prefix(3).joined(separator: " ")) exited \(process.terminationStatus): \(detail)\(log.map { "\nfull log: \($0.path)" } ?? "")")
        }
        return Output(stdout: stdout)
    }
}
#endif
