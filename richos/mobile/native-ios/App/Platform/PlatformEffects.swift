import RichOSCore
import UIKit

/// The iPhone's half of the core's effects (`EffectHandler`, stream I1's seam at 5f27785a): the
/// microphone, the recorder, playback of your own recordings, the lock haptic, iPhone Settings, and
/// Apple's notification permission. Network effects (pairing, delivery, history, reply audio) are
/// passed to `network`, the core's own handler, unchanged.
///
/// Two kinds of answer. Actions RETURNED from `handle` answer the effect that was asked (the OS's
/// microphone answer). Actions that happen later on their own — a level every 100 ms, an
/// interruption, playback ending — go through `dispatch`, which the app points at its store.
final class PlatformEffects: EffectHandler, @unchecked Sendable {
    private let network: (any EffectHandler)?
    private let clock: any Clock
    /// The reply-preview key the notification extension opens sealed previews with.
    private let previewKeys: PreviewKeyStore
    @MainActor let recorder: VoiceRecorder
    /// Where actions that were not asked for go (the store's `send`). Set once the store exists.
    @MainActor var dispatch: ((Action) -> Void)?
    /// Opens a page outside the app (the App Store listing, support, the privacy policy). Tests
    /// record it instead of leaving the app.
    @MainActor let openURL: @MainActor (URL) -> Void
    /// Apple's photo picker, camera and document browser for the + menu; `nil` without an
    /// attachment store (the platform tests).
    @MainActor private(set) var picker: AttachmentPicker?
    /// Rich's reply audio from the paired Mac (`NetworkEffects.replyAudio`); `nil` in the platform tests.
    private let replyAudio: (@Sendable (_ messageID: String, _ state: AppState) async -> Data?)?

    @MainActor
    init(network: (any EffectHandler)? = nil, clock: any Clock = SystemClock(), recorder: VoiceRecorder = VoiceRecorder(),
         previewKeys: PreviewKeyStore = .shared,
         openURL: @escaping @MainActor (URL) -> Void = { UIApplication.shared.open($0) }, attachments: URL? = nil,
         replyAudio: (@Sendable (_ messageID: String, _ state: AppState) async -> Data?)? = nil) {
        self.network = network
        self.replyAudio = replyAudio
        self.clock = clock
        self.previewKeys = previewKeys
        self.recorder = recorder
        self.openURL = openURL
        if let attachments {
            picker = AttachmentPicker(directory: attachments) { [weak self] action in self?.dispatch?(action) }
        }
        recorder.onLevel = { [weak self] level in self?.dispatch?(.voiceLevel(level)) }
        recorder.onInterrupted = { [weak self] in
            guard let self else { return }
            self.dispatch?(.voiceInterrupted(at: self.clock.nowMs()))
        }
        recorder.onPlaybackEnded = { [weak self] in self?.dispatch?(.playbackEnded) }
        recorder.onReplyProgress = { [weak self] id, progress in self?.dispatch?(.playbackProgress(id: id, progress: progress)) }
    }

    /// The courier's source of voice-message bytes (the same files the recorder writes).
    var recordings: any RecordingStore { FileRecordingStore(directory: VoiceRecorder.defaultDirectory()) }

    func recoverRecording(_ recording: KeptRecording) async throws -> KeptRecording? {
        try await Task.detached { try FileRecordingStore.recover(recording) }.value
    }

    /// The device signing keys, in the app's own Keychain group (security review I-4). Only the app
    /// signs: the Share extension passes no signed connection (`ShareViewController`, `transport:
    /// nil`) and leaves every share for the app to send, and the notification extension reads only
    /// the reply-preview key, which stays in the shared group. A key an earlier build left in the
    /// shared group (the default then) is moved on first use.
    static func identityStore() -> KeychainIdentityStore {
        KeychainIdentityStore(accessGroup: PlatformIdentity.appOnlyKeychainGroup, legacyAccessGroup: PlatformIdentity.keychainGroup)
    }

    /// What the app sends on launch and every time it becomes active, so the core's microphone state
    /// is always the OS's (PRD §3: no parallel permission state). Wired in `App/App` (stream I1).
    static func permissionMirror() -> [Action] {
        [.microphonePermission(MicrophonePermission.current())]
    }

    func handle(_ effect: Effect, state: AppState) async -> [Action] {
        switch effect {
        case .requestMicrophone:
            return [.microphonePermission(await MicrophonePermission.request())]

        case .startRecording(let id):
            let started: Bool = await MainActor.run {
                do {
                    try recorder.start(id: id)
                    return true
                } catch {
                    return false
                }
            }
            // The recorder could not start (the audio session was refused, the disk is full): the
            // gesture ends as if the system took the touch away, and nothing is kept, because
            // nothing was recorded. A denied permission is reported as the permission it is.
            guard started else { return [.voiceStartFailed(id: id), .microphonePermission(MicrophonePermission.current())] }
            return []

        case .stopRecording(_, let keep):
            await MainActor.run { recorder.stop(keep: keep) }
            return []

        case .deleteRecording(let id):
            await MainActor.run { recorder.delete(id: id) }
            return []

        case .playRecording(let id):
            await MainActor.run { recorder.play(id: id) }
            return []

        case .fetchReplyAudio(let id):
            // "Hear it": the Mac reads the reply (round 12.1 `conv-preparing-reply`), then it plays
            // (`conv-playing-reply`). Asked once; a refusal or no answer puts "Hear it" back.
            await MainActor.run { recorder.awaitReply(id) }
            let bytes = await replyAudio?(id, state)
            return await MainActor.run { () -> [Action] in
                // Stopped, or another reply asked for, while the Mac answered: nothing plays.
                guard recorder.awaitedReply == id else { return [] }
                guard let bytes, recorder.playReply(bytes, id: id) else {
                    recorder.stopPlayback()
                    return [.playbackEnded]
                }
                return [.playbackStarted(id: id)]
            }

        case .stopAudio:
            await MainActor.run { recorder.stopPlayback() }
            // Rich's reply audio is the network handler's; it hears the stop too.
            return await network?.handle(effect, state: state) ?? []

        case .hapticTick:
            await MainActor.run { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
            return []

        case .openSystemSettings:
            await MainActor.run {
                if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
            }
            return []

        case .requestNotifications:
            // Apple's question first; registering with the Mac is the network handler's, once there
            // is a token (`NotificationPlatform.onToken`).
            let status = await NotificationPlatform.shared.authorize()
            guard status == .turningOn else { return [.notificationsResult(status)] }
            return await network?.handle(effect, state: state) ?? [.notificationsResult(status)]

        case .withdrawNotifications:
            // Notifications off, or the Mac forgotten: what is already delivered goes too (D04).
            await MainActor.run { NotificationPlatform.shared.withdrawAll() }
            return []

        case .openAppStore, .openSupport, .openPrivacyPolicy:
            // The addresses live in one place, `AppLinks`; until the CEO fills them they are marked
            // placeholders (App Store listing drafts, blockers 1-4).
            if let url = AppLinks.destination(of: effect) { await MainActor.run { openURL(url) } }
            return []

        case .presentPicker(let source, let maxCount):
            await MainActor.run { picker?.present(source, maxCount: maxCount, limits: state.attachmentLimits) }
            return []

        case .forgetIdentity:
            // "Forget this phone": the preview key goes with the pairing, here on the phone. The
            // unregistration that would stop the Mac sealing previews is best effort (sent only while
            // a connection exists), so without this a phone showing no pairing would keep opening
            // them (security review I-1). The signing key is the core's to remove.
            try? previewKeys.erase()
            return await network?.handle(effect, state: state) ?? []

        case .persist, .pair, .confirmFingerprint, .checkMacConfirmation, .deliver, .loadOlder, .connect, .reconnect, .disconnect, .deleteAttachments,
             .unregisterNotifications, .fetchRichFile, .findRichFile, .stopRichFile:
            return await network?.handle(effect, state: state) ?? []
        }
    }
}
