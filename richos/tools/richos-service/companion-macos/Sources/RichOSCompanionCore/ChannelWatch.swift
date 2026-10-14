import Foundation

/// One side of the recording stopped or came back, said ONCE per change (not once a second).
///
/// The health tick has always written a red row when a side is starved (`MixDecision`
/// silence-fills it and the recording keeps growing), but nothing told the app: the byte-growth
/// watchdog stays quiet because the live side keeps the WAV growing. Found by the second review of
/// 2230466eb (rv-20261013T090233Z-2230466e-9d08): microphone callbacks stopped after five seconds
/// while system audio continued gave 21 red health rows and zero JSON alarms, then a normal
/// `closed`. The app needs the change to say "Only part of this meeting was recorded" instead of
/// presenting a confident complete summary (Granola PRD V4).
public enum CaptureChannel: String, Equatable {
    case microphone
    case systemAudio = "system-audio"
}

public struct ChannelChange: Equatable {
    public let channel: CaptureChannel
    /// true: the side stopped delivering audio (silence-filled from here); false: it resumed.
    public let stalled: Bool

    public init(channel: CaptureChannel, stalled: Bool) {
        self.channel = channel
        self.stalled = stalled
    }

    /// The sentence the app shows and the brief carries.
    public var message: String {
        switch (channel, stalled) {
        case (.microphone, true): return "The microphone stopped sending audio; your side is silent from here."
        case (.microphone, false): return "The microphone is sending audio again."
        case (.systemAudio, true): return "System audio stopped arriving; the other side is silent from here (the permission may have been turned off)."
        case (.systemAudio, false): return "System audio is arriving again."
        }
    }
}

/// The microphone running but delivering exact digital silence: what macOS gives a process whose
/// microphone access is off. A real microphone is never exactly zero for seconds on end (its own
/// noise floor), so `seconds` of peak == 0 is said once, and said again only after sound has
/// returned. Measured 2026-10-13 (end-detection run 2): the capture's microphone side was exactly
/// zero for all 195 s while a known track played into it, and every health row said green.
///
/// ONLY A MICROPHONE THAT HAS NEVER GIVEN SOUND. Access that is off gives zeros from the first
/// sample; a microphone that has been heard and then goes digitally silent is a device gating
/// itself (the installed-app walk's virtual microphone between phrases, run 8, 2026-10-13, which
/// put a false "may be turned off" line at the top of the brief), not a missing grant.
public struct MicSilence {
    private let seconds: Int
    private var zeroTicks = 0
    private var said = false
    private var heard = false

    public init(seconds: Int = 10) { self.seconds = seconds }

    /// One health tick's microphone peak. Returns the change to say, if any.
    public mutating func update(micPeak: Double, micRunning: Bool) -> String? {
        guard micRunning else { zeroTicks = 0; return nil }
        if micPeak == 0 && heard { return nil }
        if micPeak == 0 {
            zeroTicks += 1
            if zeroTicks == seconds && !said {
                said = true
                return MicSilence.silent
            }
            return nil
        }
        zeroTicks = 0
        heard = true
        if said {
            said = false
            return MicSilence.back
        }
        return nil
    }

    public static let silent = "The microphone is giving RichOS only silence; microphone access may be turned off for RichOS in System Settings."
    public static let back = "The microphone is sending audio again."
}

/// Fed every health tick; returns only the changes.
public struct ChannelWatch {
    private var micDown = false
    private var systemDown = false

    public init() {}

    /// - micDown: the microphone side is starved (`MixDecision.micStarved`) or its input is not running.
    /// - systemDown: the system side is starved or the tap's IO is not running.
    public mutating func update(micDown mic: Bool, systemDown sys: Bool) -> [ChannelChange] {
        var out: [ChannelChange] = []
        if mic != micDown { out.append(ChannelChange(channel: .microphone, stalled: mic)); micDown = mic }
        if sys != systemDown { out.append(ChannelChange(channel: .systemAudio, stalled: sys)); systemDown = sys }
        return out
    }
}
