import Foundation
import RichOSCompanionCore

/// The live capture loop: wire the Core Audio engine to the SessionWriter, run a 10 Hz mix pump and
/// a 1 Hz health tick + watchdog, and finalize cleanly on SIGINT (or SIGTERM). This is the
/// outside-the-browser watchdog made real (§6.3) — it holds its own timer and alarms on stalls the
/// capture can't hide.
///
/// ORDER (2026-10-13): the tap is made first and its real sample rate read; only then is the
/// session written, so the WAV header states the rate the audio was recorded at. A tap that cannot
/// be made therefore leaves NO session directory: there is nothing recorded to be loud about, and
/// the failure is reported (stderr, and a `failed` line to the app in `--json` mode).
@available(macOS 14.4, *)
func runLiveCapture(zone: String, startedAt: Int64, excludePids: [Int32]) {
    let capture = CoreAudioCapture(excludePids: excludePids)
    do {
        try capture.prepare()
    } catch {
        let message = "capture failed to start: \(error.localizedDescription). If this is the first run, "
            + "grant \"System Audio Recording\" and Microphone in System Settings > Privacy & Security, then retry."
        Notifier.alarm(message)
        Notifier.event("failed", [("message", .string(message)), ("dir", .null)])
        exit(1)
    }

    // session.json is written at START (§6.1) — a call that captures nothing after this point
    // leaves a loud open/empty session on disk, never an absence.
    let params = makeParams(startedAt: startedAt, sampleRate: capture.sampleRate)
    let writer = SessionWriter(rootZone: zone, params: params, clock: nowMs)
    do { try writer.start() } catch {
        let message = "could not create the session directory: \(error.localizedDescription)"
        Notifier.alarm(message)
        Notifier.event("failed", [("message", .string(message)), ("dir", .null)])
        capture.stop()
        exit(1)
    }
    let dir = (zone as NSString).appendingPathComponent(params.sessionId)
    Notifier.info("session (open) written: \(dir)")

    do {
        try capture.run(writer: writer)
        Notifier.info("capturing system audio + mic at \(capture.sampleRate) Hz. Send SIGINT to stop.")
        Notifier.event("started", [("dir", .string(dir)), ("sessionId", .string(params.sessionId)),
                                   ("sampleRate", .int(capture.sampleRate))])
    } catch {
        // The open session.json stays on disk as the anomaly; we exit loud, not silent.
        let message = """
        capture failed to start: \(error.localizedDescription). \
        If this is the first run, grant "System Audio Recording" and Microphone in \
        System Settings > Privacy & Security, then retry. The open session at \(dir) is the on-disk \
        record that this call was NOT captured.
        """
        Notifier.alarm(message)
        Notifier.event("failed", [("message", .string(message)), ("dir", .string(dir))])
        capture.stop()
        exit(1)
    }

    let timerQueue = DispatchQueue(label: "com.richos.companion.timers")

    // 10 Hz mix pump: keep the WAV growing and both sources frame-aligned.
    let pump = DispatchSource.makeTimerSource(queue: timerQueue)
    pump.schedule(deadline: .now() + 0.1, repeating: 0.1)
    pump.setEventHandler { _ = try? writer.pump() }
    pump.resume()

    // 1 Hz health tick + fsync + watchdog escalation + RichOS's own speech kept out of the tap.
    var lastBytes = 0
    var stalledSeconds = 0
    var systemAudioSeen = false
    var channels = ChannelWatch()
    let health = DispatchSource.makeTimerSource(queue: timerQueue)
    health.schedule(deadline: .now() + 1.0, repeating: 1.0)
    health.setEventHandler {
        let (mp, mr, sp, sr) = capture.snapshotLevels()
        try? writer.writeHealthTick(
            micPeak: mp, micRms: mr, sysPeak: sp, sysRms: sr,
            tapRunning: capture.tapRunning, micRunning: capture.micRunning)
        try? writer.flush() // durable to disk each second (§6.5)
        if let change = capture.refreshExclusions() { Notifier.info(change) }
        // A side that stalls (or comes back) is said once, to stderr and to the app: the byte
        // watchdog below cannot see it, because the live side keeps the recording growing.
        let decision = writer.lastDecision
        for change in channels.update(micDown: (decision?.micStarved ?? false) || !capture.micRunning,
                                      systemDown: (decision?.systemStarved ?? false) || !capture.tapRunning) {
            if change.stalled { Notifier.alarm(change.message) } else { Notifier.info(change.message) }
            Notifier.event("channel", [("channel", .string(change.channel.rawValue)),
                                       ("stalled", .bool(change.stalled)),
                                       ("message", .string(change.message))])
        }
        // Sound arrived through the system tap: the System Audio Recording grant exists (macOS
        // has no API to ask). Said once; the app may now show the voice bar's live level.
        if !systemAudioSeen && sp > 0 {
            systemAudioSeen = true
            Notifier.event("system-audio", [("flowing", .bool(true))])
        }

        // Watchdog: byte growth is the ground-truth liveness signal (§6.2 "no growth > 15 s").
        let bytes = writer.pumpedBytesTotal
        if bytes == lastBytes { stalledSeconds += 1 } else { stalledSeconds = 0 }
        lastBytes = bytes
        if stalledSeconds == 15 {
            Notifier.alarm("no audio has been written for 15s — the capture has stalled (tap stopped, "
                + "device changed, or System Audio Recording permission was revoked). Session: \(dir)")
        }
    }
    health.resume()

    // Clean finalize on SIGINT (the app's stop) or SIGTERM: drain, flush, rewrite session.json as
    // closed -> the pipeline may transcribe it.
    var sources: [DispatchSourceSignal] = []
    for sig in [SIGINT, SIGTERM] {
        signal(sig, SIG_IGN)
        let source = DispatchSource.makeSignalSource(signal: sig, queue: timerQueue)
        source.setEventHandler {
            pump.cancel(); health.cancel()
            capture.stop()
            _ = try? writer.pump()
            do {
                try writer.close()
                Notifier.info("session (closed) written: \(dir) — the pipeline may now transcribe it.")
                Notifier.event("closed", [("dir", .string(dir))])
                exit(0)
            } catch {
                let message = "failed to finalize session: \(error.localizedDescription)"
                Notifier.alarm(message)
                Notifier.event("failed", [("message", .string(message)), ("dir", .string(dir))])
                exit(1)
            }
        }
        source.resume()
        sources.append(source)
    }

    withExtendedLifetime(sources) { RunLoop.main.run() }
}
