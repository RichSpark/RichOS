package dev.richos.android.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Everything that can change [AppState]. The UI, the headless CLI and the emulator bridge
 * all produce these and all go through [RichCore.dispatch]; there is no second path.
 *
 * The JSON form is the preserved phone core's (`richos/mobile/core/app.js`), discriminated
 * by `type`: `{"type":"compose","text":"Hello"}`, `{"type":"send"}`, … so a scenario written
 * for one client reads the same on every client. `theme` is the one addition.
 */
@Serializable
sealed interface Action {
    @Serializable @SerialName("play-kept")
    data object PlayKept : Action
    @Serializable @SerialName("playback-ended")
    data class PlaybackEnded(val id: String) : Action
    /** "Hear it" under Rich's reply [id]: its audio from the Mac (`GET /api/audio`), then played. */
    @Serializable @SerialName("reply-play")
    data class HearReply(val id: String) : Action
    /** "Stop" on the reply [id] being heard. */
    @Serializable @SerialName("reply-stop")
    data class StopReply(val id: String) : Action
    /** A tap on a file Rich attached ([RichFile.id]): it comes over from the Mac, then opens. */
    @Serializable @SerialName("file-open")
    data class OpenRichFile(val id: String) : Action
    /** The × on a file coming over: it stops, and nothing half-written is kept. */
    @Serializable @SerialName("file-stop")
    data class StopRichFile(val id: String) : Action
    /** The viewer was closed, so a later arrival does not open it again by itself. */
    @Serializable @SerialName("file-viewed")
    data class RichFileViewed(val id: String) : Action
    @Serializable @SerialName("select-thread")
    data class SelectThread(val threadId: String) : Action

    @Serializable @SerialName("compose")
    data class Compose(val text: String) : Action

    @Serializable @SerialName("remember-reading")
    data class RememberReading(val anchor: ReadingAnchor? = null) : Action

    @Serializable @SerialName("send")
    data object Send : Action

    @Serializable @SerialName("answer-question")
    data class AnswerQuestion(val id: String, val options: List<String> = emptyList(), val text: String = "", val revision: Long? = null) : Action

    @Serializable @SerialName("send-voice")
    data class SendVoice(val recording: Recording, val clientId: String? = null, val threadId: String? = null) : Action

    @Serializable @SerialName("network")
    data class Network(val online: Boolean) : Action

    @Serializable @SerialName("retry")
    data object Retry : Action

    @Serializable @SerialName("sync")
    data object Sync : Action

    @Serializable @SerialName("discard")
    data class Discard(val clientId: String) : Action

    @Serializable @SerialName("theme")
    data class SetTheme(val theme: Theme) : Action

    /** A scanned or pasted pairing link (contract §2.1): sends the code to the Mac it names. */
    @Serializable @SerialName("pair")
    data class Pair(val link: String) : Action

    /** The user's answer to the six words: "They match" (true) or "They do not match" (false). */
    @Serializable @SerialName("confirm-words")
    data class ConfirmWords(val match: Boolean) : Action

    /**
     * Pairing v2: ask the Mac whether the person has pressed "They match" on it, if an ask is due
     * (the app's one timer sends this at [AppState.macWaitDueInMs]). The core decides: nothing is
     * asked while the app is hidden, before the schedule, past the bound or past 22 requests.
     */
    @Serializable @SerialName("mac-wait")
    data object MacWait : Action

    /** Forget this pairing on the phone. Refused while unsent work is waiting. */
    @Serializable @SerialName("forget")
    data object Forget : Action

    /**
     * Bytes from the Mac's event stream, exactly as they arrived, split anywhere (contract §5.4).
     * The connection owner feeds the live stream through this; a scenario or the CLI can feed a
     * recorded one, which is how the conversation is tested without a Mac.
     */
    @Serializable @SerialName("receive")
    data class Receive(val wire: String) : Action

    /** The stream owner's socket changed state: `opening`, `open` or `away`. */
    @Serializable @SerialName("link")
    data class Link(val status: LinkStatus) : Action

    /**
     * What a diagnosis found while away: the phone's own network, and the managed service. And
     * [vpn], the OS's own report of whether the default network runs through a VPN (Tailscale on
     * Android is one), from its network callback, never a probe (D05).
     */
    @Serializable @SerialName("health")
    data class Health(val phoneOnline: Boolean? = null, val service: ServiceState? = null, val vpn: Boolean? = null) : Action

    /** Time passed: republish time-derived state (the reconnecting notice, the voice timer). The app's one timer sends it. */
    @Serializable @SerialName("tick")
    data object Tick : Action

    // --- voice (round-12 groups 4, 5, 6). Names and fields are the native iOS core's. ----------

    /** Touch-down on the microphone. [id] names the recording; [width] is the composer's, in dp. */
    @Serializable @SerialName("voice-press")
    data class VoicePress(val id: String, val width: Double, val at: Long) : Action

    /** Accessibility's "record hands-free": start a recording already locked. */
    @Serializable @SerialName("voice-start-locked")
    data class VoiceStartLocked(val id: String, val width: Double, val at: Long) : Action

    /**
     * The OS answered or reports the microphone permission. [canAsk]: while denied, whether the
     * system would still show its own question if asked again (Android's
     * `shouldShowRequestPermissionRationale`); false once it will not, and always on iOS.
     */
    @Serializable @SerialName("microphone-permission")
    data class MicrophonePermission(
        val permission: Microphone,
        @OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)
        @kotlinx.serialization.EncodeDefault(kotlinx.serialization.EncodeDefault.Mode.NEVER)
        val canAsk: Boolean = false,
    ) : Action

    /** The microphone-off card's "Allow microphone": ask the system again, only while it still would. */
    @Serializable @SerialName("ask-microphone")
    data object AskMicrophone : Action

    /** The microphone-off card's "Not now": the card goes until the next press finds the microphone off. */
    @Serializable @SerialName("dismiss-microphone-card")
    data object DismissMicrophoneCard : Action

    /** The finger moved: offsets from the touch-down point, in dp (negative = left / up). */
    @Serializable @SerialName("voice-move")
    data class VoiceMove(val dx: Double, val dy: Double, val at: Long) : Action

    @Serializable @SerialName("voice-release")
    data class VoiceRelease(val at: Long) : Action

    /** Tap on the send circle while locked. */
    @Serializable @SerialName("voice-locked-send")
    data class VoiceLockedSend(val at: Long) : Action

    /** Tap on Cancel while locked. */
    @Serializable @SerialName("voice-locked-cancel")
    data class VoiceLockedCancel(val at: Long) : Action

    /** The system took the touch away (an alert over the app). */
    @Serializable @SerialName("voice-touch-canceled")
    data class VoiceTouchCanceled(val at: Long) : Action

    /** The app left the screen or the OS took the audio: the recording is kept, never sent. */
    @Serializable @SerialName("voice-interrupted")
    data class VoiceInterrupted(val at: Long) : Action

    /** The recording's level, 0 to 1, sampled every 100 ms. */
    @Serializable @SerialName("voice-level")
    data class VoiceLevel(val level: Double) : Action

    /** The end animation finished. */
    @Serializable @SerialName("voice-settled")
    data object VoiceSettled : Action

    /** Send a kept recording (`rec-card`). */
    @Serializable @SerialName("send-kept")
    data class SendKept(val id: String, val at: Long = 0) : Action

    /** Let a kept recording go. */
    @Serializable @SerialName("discard-kept")
    data class DiscardKept(val id: String) : Action

    // --- notifications, settings, update notices (round-12 groups 8, 9, 10; iOS names) -----------

    @Serializable @SerialName("turn-on-notifications")
    data object TurnOnNotifications : Action

    /** What the OS and the Mac said to a registration (or its absence). */
    @Serializable @SerialName("notifications-result")
    data class NotificationsResult(val status: NotificationStatus) : Action

    @Serializable @SerialName("turn-off-notifications")
    data object TurnOffNotifications : Action

    /** `notif-offer`'s "Not now": asked once. */
    @Serializable @SerialName("dismiss-notification-offer")
    data object DismissNotificationOffer : Action

    @Serializable @SerialName("set-previews")
    data class SetPreviews(val on: Boolean) : Action

    @Serializable @SerialName("open-sheet")
    data class OpenSheet(val sheet: Sheet) : Action

    @Serializable @SerialName("close-sheet")
    data object CloseSheet : Action

    /** Settings' red row: opens `settings-forget`, or `settings-forget-blocked` while unsent work waits. */
    @Serializable @SerialName("forget-pairing")
    data object ForgetPairing : Action

    /** The forget sheet's confirmation: notifications off first, then the key and the pairing go. */
    @Serializable @SerialName("confirm-forget")
    data object ConfirmForget : Action

    /**
     * The refused forget's way out: what waits was written for this pairing's Mac, which may be gone
     * for good, so it is discarded and the pairing forgotten as [ConfirmForget] does.
     */
    @Serializable @SerialName("discard-and-forget")
    data object DiscardAndForget : Action

    @Serializable @SerialName("open-system-settings")
    data object OpenSystemSettings : Action

    /** A notification was tapped: open that conversation and let that reply glow once (`conv-focused`). */
    @Serializable @SerialName("opened-from-notification")
    data class OpenedFromNotification(val messageId: String, val threadId: String? = null) : Action

    /** A tapped notification's own words, shown at once as a provisional reply until the stream's row replaces them. */
    @Serializable @SerialName("provisional-reply")
    data class ProvisionalReply(val threadId: String, val event: String, val text: String) : Action

    @Serializable @SerialName("clear-focus")
    data object ClearFocus : Action

    /** The hosted policy's answer: a notice (or none) and whether voice is paused by policy. */
    @Serializable @SerialName("update-policy")
    data class UpdatePolicy(val notice: UpdateNotice? = null, val voicePaused: Boolean = false) : Action

    /** A banner or a dialog can be dismissed; a required update cannot. */
    @Serializable @SerialName("dismiss-update")
    data object DismissUpdate : Action

    @Serializable @SerialName("open-app-store")
    data object OpenAppStore : Action

    @Serializable @SerialName("open-support")
    data object OpenSupport : Action

    /** Settings, "Check for updates": on Android updates come from Google Play, so it opens the listing. */
    @Serializable @SerialName("check-for-updates")
    data object CheckForUpdates : Action

    /** Settings, "Privacy policy", and the consent screen's "Learn more" (Google Play: a link inside the app). */
    @Serializable @SerialName("open-privacy-policy")
    data object OpenPrivacyPolicy : Action

    /** The platform's push token arrived (after `turn-on-notifications`): register it with the Mac. */
    @Serializable @SerialName("push-token")
    data class PushToken(val token: String, val previewKey: String? = null) : Action

    /** Scrolled to the top: fetch the next older chunk (contract §5.5; no pagination, chunked loading). */
    @Serializable @SerialName("load-older")
    data object LoadOlder : Action

    /** Photos and files picked or shared, staged and hashed by the platform: into the composer. */
    @Serializable @SerialName("attach")
    data class Attach(val files: List<Attachment>) : Action

    /** Take one picked file back out of the composer. */
    @Serializable @SerialName("remove-attachment")
    data class RemoveAttachment(val id: String) : Action

    /** The + menu's Photos, Camera or Files (`att-menu`): the Mac's limits decide, then the platform's picker. */
    @Serializable @SerialName("pick-attachments")
    data class PickAttachments(val source: AttachSource) : Action

    /** The platform refused an item before staging it (over the Mac's per-file limit while copying). */
    @Serializable @SerialName("attach-refused")
    data class AttachRefused(val name: String, val bytes: Long? = null, val tooLarge: Boolean = true) : Action

    /** The camera permission was refused, or no camera app can take the photo (`att-denied-camera`). */
    @Serializable @SerialName("attach-permission-denied")
    data class AttachPermissionDenied(val source: AttachSource) : Action

    /** "Not now" / "Got it" on a photos-and-files card. */
    @Serializable @SerialName("dismiss-attach-notice")
    data object DismissAttachNotice : Action

    /**
     * Share to Rich from another app (CEO §75): words and/or staged files, sent as one message
     * WITHOUT touching the composer's draft or its pending attachments. [clientId] is chosen by
     * the caller so it can watch that message until the Mac accepts it.
     */
    @Serializable @SerialName("share")
    data class Share(val clientId: String, val text: String = "", val files: List<Attachment> = emptyList()) : Action

    /** Photos and files to Rich (CEO §75), already staged and hashed by the platform. */
    @Serializable @SerialName("send-attachments")
    data class SendAttachments(val files: List<Attachment>, val text: String = "") : Action
}

@Serializable
data class Recording(val id: String, val seconds: Double)

/** A refused action or command. The message is a sentence a person can act on. */
class CoreError(message: String) : IllegalArgumentException(message)
