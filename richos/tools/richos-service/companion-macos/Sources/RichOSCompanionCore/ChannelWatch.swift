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
