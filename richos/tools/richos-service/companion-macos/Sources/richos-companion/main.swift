import Foundation
import RichOSCompanionCore

let VERSION = "0.1.0-p2"

// MARK: - Arg parsing helpers

func flag(_ name: String, _ args: [String]) -> String? {
    guard let i = args.firstIndex(of: name), i + 1 < args.count else { return nil }
    return args[i + 1]
}
func nowMs() -> Int64 { Int64(Date().timeIntervalSince1970 * 1000) }

/// The RichOS checkout this binary is running from, if any. `nil` for a copied-elsewhere binary,
/// which is not an error — there is then no repository for the zone to be inside of.
func productRepo() -> String? {
    let start = URL(fileURLWithPath: CommandLine.arguments[0])
        .resolvingSymlinksInPath().deletingLastPathComponent().path
    return DropZone.locateProductRepo(startingAt: start) {
        FileManager.default.fileExists(atPath: $0)
    }
}

/// Resolve the drop zone the pipeline watches. The rule, and why it is a mirror of
/// `config.js#dropZone` rather than a second opinion, is documented on `DropZone` — including the
/// 2026-08-29 finding that this function used to default INSIDE the public product repo, into a path
/// the pipeline refuses to read.
///
/// Refusal is fatal here on purpose: the alternative is recording a call the pipeline will never
/// transcribe, which is precisely the silent absence §6.1 exists to prevent.
func resolveZone(_ explicit: String?) -> String {
    resolveZoneDetail(explicit).path
}

func resolveZoneDetail(_ explicit: String?) -> DropZone.Resolution {
    do {
        return try DropZone.resolve(
            explicit: explicit,
            env: ProcessInfo.processInfo.environment,
            home: NSHomeDirectory(),
            productRepo: productRepo()
        )
    } catch {
        FileErr.write("\(error)\n")
        exit(1)
    }
}

func makeParams(startedAt: Int64, sampleRate: Int) -> SessionContract.Params {
    let sessionId = SessionContract.sessionDirName(startedAt: startedAt, platformId: "system", slug: "call")
    return SessionContract.Params(
        sessionId: sessionId, startedAt: startedAt, sampleRate: sampleRate, chunkMs: 3000,
        micEnabled: true, captureTarget: "system", platformId: "system",
        platformLabel: "Desktop call", platformSlug: "call", companionVersion: VERSION, processHint: nil
    )
}

// MARK: - Subcommands

func cmdDoctor() {
    let pi = ProcessInfo.processInfo
    let v = pi.operatingSystemVersion
    print("RichOS macOS capture companion \(VERSION)")
    print("macOS: \(v.majorVersion).\(v.minorVersion).\(v.patchVersion)")
    let tapOK = v.majorVersion > 14 || (v.majorVersion == 14 && v.minorVersion >= 4)
    print("Core Audio process tap (>= 14.4): \(tapOK ? "AVAILABLE" : "UNAVAILABLE — needs macOS 14.4+")")
    // Resolved WITHOUT exiting on refusal: `doctor` exists to report a broken environment, so it
    // must be able to print the refusal rather than die with it.
    let repo = productRepo()
    print("Product repo: \(repo ?? "not inside a RichOS checkout")")
    do {
        let z = try DropZone.resolve(
            explicit: nil, env: pi.environment, home: NSHomeDirectory(), productRepo: repo)
        print("Drop zone: \(z.path)")
        print("  source: \(z.source.rawValue)\(z.company.map { " (company: \($0))" } ?? "")")
        if z.source == .corpus {
            print("  (set RICHOS_DROP_ZONE, or LORO_CORPUS, to put sessions somewhere else)")
        }
        if !FileManager.default.fileExists(atPath: z.path) {
            print("  NOTE: does not exist yet — `capture` creates it on first run.")
        }
    } catch {
        print("Drop zone: REFUSED — \(error)")
    }
    for bin in ["ffmpeg", "whisper-cli"] {
        let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        p.arguments = ["which", bin]
        let pipe = Pipe(); p.standardOutput = pipe; p.standardError = FileHandle.nullDevice
        try? p.run(); p.waitUntilExit()
        let out = String(decoding: pipe.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        print("pipeline dep \(bin): \(out.isEmpty ? "not found (P1 pipeline needs it)" : out)")
    }
    print("")
    print("PERMISSION GATE: the first `capture` run triggers the macOS \"System Audio Recording\"")
    print("prompt (System Settings > Privacy & Security > System Audio Recording) + a Microphone")
    print("prompt. Both are human GUI grants; there is no non-interactive path and no API to query")
    print("current status. Grant once, then capture runs unattended.")
}

/// Headless proof path: push a pre-made sample through the SAME SessionWriter/ChannelMixer/WavWriter
/// the live capture uses, producing a valid contract dir WITHOUT any TCC grant. `--stereo` routes a
/// stereo WAV's L->mic, R->system; `--mic`/`--system` take two mono WAVs (proves the L/R mapping).
func cmdIngest(_ args: [String]) throws {
    let zone = resolveZone(flag("--zone", args))
    let startedAt = flag("--started-at", args).flatMap { Int64($0) } ?? nowMs()

    var micCh: [Float] = []
    var sysCh: [Float] = []
    var rate = 48_000

    if let stereo = flag("--stereo", args) {
        let pcm = try WavReader.read(path: stereo)
        rate = pcm.sampleRate
        micCh = pcm.channelsData.first ?? []
        sysCh = pcm.channelsData.count > 1 ? pcm.channelsData[1] : [Float](repeating: 0, count: micCh.count)
    } else if let micPath = flag("--mic", args), let sysPath = flag("--system", args) {
        let m = try WavReader.read(path: micPath)
        let s = try WavReader.read(path: sysPath)
        rate = m.sampleRate
        micCh = m.channelsData.first ?? []
        sysCh = s.channelsData.first ?? []
    } else {
        FileErr.write("ingest requires --stereo <wav> OR --mic <wav> --system <wav>\n")
        exit(2)
    }

    let params = makeParams(startedAt: startedAt, sampleRate: rate)
    // Deterministic clock so the ingest is reproducible: advance by real audio duration.
    var virtualNow = startedAt
    let writer = SessionWriter(rootZone: zone, params: params, clock: { virtualNow })
    try writer.start()
    print("session (open) written: \((zone as NSString).appendingPathComponent(params.sessionId))")

    // Feed in 1-second blocks so health ticks + pumps mirror a live 1 Hz cadence.
    let block = rate
    var offset = 0
    let total = min(micCh.count, sysCh.count) > 0 ? max(micCh.count, sysCh.count) : 0
    while offset < total {
        let end = min(offset + block, total)
        let micSlice = offset < micCh.count ? Array(micCh[offset..<min(end, micCh.count)]) : []
        let sysSlice = offset < sysCh.count ? Array(sysCh[offset..<min(end, sysCh.count)]) : []
        writer.pushMic(micSlice)
        writer.pushSystem(sysSlice)
        try writer.pump()
        try writer.writeHealthTick(
            micPeak: ChannelMixer.peak(micSlice), micRms: ChannelMixer.rms(micSlice),
            sysPeak: ChannelMixer.peak(sysSlice), sysRms: ChannelMixer.rms(sysSlice),
            tapRunning: true, micRunning: true)
        virtualNow += 1000
        offset = end
    }
    try writer.close()
    let dir = (zone as NSString).appendingPathComponent(params.sessionId)
    print("session (closed) written: \(dir)")
    print("run the P1 pipeline over it:  node ../bin/richos-service.js run \"\(dir)\"")
}

func cmdCapture(_ args: [String]) {
    let zone = resolveZone(flag("--zone", args))
    let startedAt = nowMs()

    // COORDINATION (§5.4): before starting an all-system capture, ask the SHARED authority whether a
    // browser call is already owned by the extension. If so, stand down (the extension's browser-tab
    // capture is richer) unless explicitly overridden — never double-capture. Best-effort: if the
    // service is absent the companion proceeds, exactly like the extension's Downloads fallback.
    let kind = flag("--kind", args) ?? Coordination.defaultCaptureKind
    let processHint = flag("--process-hint", args)
    let provisionalId = SessionContract.sessionDirName(startedAt: startedAt, platformId: "system", slug: "call")
    let force = args.contains("--force")
    if let decision = CompanionCoordinator.consultClaim(zone: zone, captureKind: kind, processHint: processHint, sessionId: provisionalId) {
        if decision.decision == .standDown && !force {
            FileErr.write("stand down: \(decision.reason)\n")
            if let ex = decision.excludeProcessHint { FileErr.write("  (the extension owns \(ex); it will capture this call with captions/names)\n") }
            FileErr.write("  pass --force to capture anyway (double-capture; the pipeline dedup backstop keeps the richer one).\n")
            exit(0)
        }
    }

    if #available(macOS 14.4, *) {
        runLiveCapture(zone: zone, startedAt: startedAt)
    } else {
        FileErr.write("capture requires macOS 14.4+ (Core Audio process taps).\n")
        exit(1)
    }
}

func usage() {
    print("""
    RichOS macOS capture companion \(VERSION)

    USAGE:
      richos-companion doctor
          Report environment + the permission gate.
      richos-companion capture [--zone DIR]
          Live capture: system-audio tap + mic -> 2-channel contract dir (needs the TCC grant).
      richos-companion ingest --stereo <wav> [--zone DIR] [--started-at MS]
      richos-companion ingest --mic <monoWav> --system <monoWav> [--zone DIR]
          Headless: push a sample through the real contract writer (no grant needed) -> a valid
          session dir the P1 pipeline transcribes.
      richos-companion watch [--exclude-pid N]... [--exclude-bundle ID]... [--poll S] [--trace]
          Report other processes' microphone use as JSON lines (no permission needed). Exits when
          stdin closes. The app's meeting prompts and end-of-call detection read it.

    The drop zone defaults to $RICHOS_DROP_ZONE, else <checkout>/wiki/raw/meetings.
    """)
}

// MARK: - Dispatch

let argv = Array(CommandLine.arguments.dropFirst())
guard let cmd = argv.first else { usage(); exit(2) }
let rest = Array(argv.dropFirst())
do {
    switch cmd {
    case "doctor": cmdDoctor()
    case "ingest": try cmdIngest(rest)
    case "capture": cmdCapture(rest)
    case "watch": cmdWatch(rest)
    case "-h", "--help", "help": usage()
    default:
        FileErr.write("unknown command: \(cmd)\n")
        usage()
        exit(2)
    }
} catch {
    FileErr.write("error: \(error.localizedDescription)\n")
    exit(1)
}
