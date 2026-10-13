import Foundation

/// Where the microphone's audio ends on the host clock, so a microphone restarted on a new input
/// device keeps the CEO's side lined up with the call's side.
///
/// Plugging in a headset mid-call changes the default input; the capture restarts its microphone on
/// the new device (`CoreAudioCapture`), and the time between the old device's last buffer and the
/// new device's first one is audio the microphone never delivered. `SessionWriter` pairs the two
/// sides frame by frame, so that time must become silence on the microphone side, or every later
/// word of the CEO's would sit that much late against the other side for the rest of the call.
///
/// Times are host-clock seconds of each buffer's first sample (`AVAudioTime.hostTime`), so the gap
/// is measured, not guessed from how long the restart took.
public struct MicTimeline {
    private var endSeconds: Double?

    public init() {}

    /// One microphone buffer, `seconds` long, whose first sample is at `startSeconds`.
    /// `afterRestart`: the first buffer from a restarted input.
    /// Returns the seconds of audio the microphone missed before this buffer: 0 for an ordinary
    /// buffer (consecutive buffers are never padded, so clock jitter cannot add drift), and 0 when a
    /// restarted input's first buffer overlaps where the last one ended.
    public mutating func buffer(startSeconds: Double, seconds: Double, afterRestart: Bool) -> Double {
        defer { endSeconds = startSeconds + seconds }
        guard afterRestart, let end = endSeconds else { return 0 }
        return max(0, startSeconds - end)
    }
}
