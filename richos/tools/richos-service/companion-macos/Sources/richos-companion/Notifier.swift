import Foundation
import RichOSCompanionCore

/// Loud, out-of-band alerting for the never-silent guarantee (§6.3/§6.4). The companion IS the
/// outside-the-browser watchdog, so when it detects a stall it must surface it where a human sees it
/// even if a call app is wedged: stderr always, plus a macOS Notification Center banner via
/// `osascript` (no extra entitlement, works from a CLI). The chime stays OFF by default — an open
/// mic would pick it up (§6.4).
///
/// APP MODE (`capture --json`, Granola PRD V4: "surface capture failure in the app instead of
/// leaving it only on stderr"): every alarm and state change is ALSO one JSON line on stdout for
/// the app that started the capture, and no banner is posted, because the app shows the failure
/// beside the recording indicator itself. Lines:
///   {"type":"started","at":ms,"dir":..,"sessionId":..,"sampleRate":n}
///   {"type":"alarm","at":ms,"message":..}
///   {"type":"info","at":ms,"message":..}
///   {"type":"closed","at":ms,"dir":..}
///   {"type":"failed","at":ms,"message":..,"dir":..|null}
///   {"type":"stand-down","at":ms,"reason":..}
///   {"type":"channel","at":ms,"channel":"microphone"|"system-audio","stalled":bool,"message":..}
///     once when a side stops delivering audio (silence-filled from then) and once when it resumes
///   {"type":"system-audio","at":ms,"flowing":true}   once, when sound first arrives through the tap
/// The app holds the capture's stdin (`--end-with-stdin`): when it closes, the capture finishes as
/// on SIGINT, and these lines then go nowhere, quietly.
enum Notifier {
    /// Set by `capture --json`.
    static var appMode = false

    static func alarm(_ message: String) {
        FileErr.write("‼️  RichOS companion ALARM: \(message)\n")
        if appMode { event("alarm", [("message", .string(message))]) }
        else { notify(title: "RichOS capture alarm", message: message) }
    }

    static func info(_ message: String) {
        FileErr.write("•  \(message)\n")
        if appMode { event("info", [("message", .string(message))]) }
    }

    /// One JSON line on stdout, in app mode only.
    static func event(_ type: String, _ fields: [(String, JSON)]) {
        guard appMode else { return }
        let line = JSON.object([("type", .string(type)), ("at", .int(Int(Date().timeIntervalSince1970 * 1000)))] + fields)
        // `try?`, not `write(_:)`: the app may have gone (quit, crash) while the recording finishes,
        // and the old call raises an exception on the closed pipe that would end this process.
        try? FileHandle.standardOutput.write(contentsOf: Data((line.compact() + "\n").utf8))
    }

    private static func notify(title: String, message: String) {
        let script = "display notification \(quote(message)) with title \(quote(title))"
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        p.arguments = ["-e", script]
        p.standardError = FileHandle.nullDevice
        p.standardOutput = FileHandle.nullDevice
        try? p.run()
    }

    private static func quote(_ s: String) -> String {
        "\"" + s.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"") + "\""
    }
}

enum FileErr {
    static func write(_ s: String) {
        try? FileHandle.standardError.write(contentsOf: Data(s.utf8))
    }
}
