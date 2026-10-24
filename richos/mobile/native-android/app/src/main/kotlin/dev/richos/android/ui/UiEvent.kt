package dev.richos.android.ui

import dev.richos.android.core.Action
import dev.richos.android.core.AttachSource
import dev.richos.android.core.Sheet

/**
 * Everything a person can do on a screen. The screens emit these and nothing else; [toAction]
 * turns each into core's [Action] where core has one, so a tap and the command line reach the
 * same code (richos/mobile/AGENTS.md: "UI code invokes the same actions as the CLI").
 *
 * Events with no core action yet return null from [toAction] and are listed for stream A1 in
 * [NOT_YET_IN_CORE]; the screens still emit them, so wiring one is a one-line change here.
 */
sealed interface UiEvent {
    data class AnswerQuestion(val id: String, val options: List<String>, val text: String, val revision: Long?) : UiEvent
    // --- the composer -------------------------------------------------------------------------
    data class Draft(val text: String) : UiEvent
    data object SendText : UiEvent

    // --- the outbox ---------------------------------------------------------------------------
    data object TryNow : UiEvent
    data class Discard(val clientId: String) : UiEvent

    // --- voice: raw finger events; core owns every threshold and decides what they mean ----------
    /** Touch-down: [id] names the new recording, [widthDp] is the composer's, [at] epoch ms. */
    data class VoiceDown(val id: String, val widthDp: Float, val at: Long) : UiEvent
    /** Finger travel from the touch-down point in dp; negative is left / up. */
    data class VoiceMove(val dxDp: Float, val dyDp: Float, val at: Long) : UiEvent
    data class VoiceUp(val at: Long) : UiEvent
    /** The system took the touch away (another gesture, a dialog). Never a send. */
    data class VoiceTouchCanceled(val at: Long) : UiEvent
    data class VoiceCancelTapped(val at: Long) : UiEvent
    /** TalkBack's double-tap on the microphone: a hands-free (locked) recording, no hold needed. */
    data class VoiceStartHandsFree(val id: String, val widthDp: Float, val at: Long) : UiEvent
    data class VoiceSendTapped(val at: Long) : UiEvent
    /** A transition the screen plays after core ended the recording has finished. */
    data object VoiceSettled : UiEvent
    data class RecordingSend(val id: String) : UiEvent
    data class RecordingDiscard(val id: String) : UiEvent
    data object RecordingPlay : UiEvent

    // --- the conversation ---------------------------------------------------------------------
    data class PlayVoice(val messageId: String) : UiEvent
    data class HearReply(val messageId: String) : UiEvent
    data class StopReply(val messageId: String) : UiEvent
    /** A file Rich attached (round 36 screen 3): tapped, its × while it comes over, its viewer closed. */
    data class OpenRichFile(val fileId: String) : UiEvent
    data class StopRichFile(val fileId: String) : UiEvent
    data class CloseRichFile(val fileId: String) : UiEvent
    /** The reader reached the oldest loaded message; core loads the next chunk (no pagination). */
    data object NearOldest : UiEvent
    data class Reading(val anchor: dev.richos.android.core.ReadingAnchor?) : UiEvent

    // --- pairing ------------------------------------------------------------------------------
    // Scan, Pair again, Close the scanner and Discard and pair are the PLATFORM's (the camera, the
    // scanner on screen): `ui/pairing/PairingEntry` handles them before [toAction] is asked, and a
    // code it reads reaches core as `pair`, the action a pasted link reaches.
    data object ScanCode : UiEvent
    data object UsePairingLink : UiEvent
    /** A pairing link, scanned or pasted: core pairs with it. */
    data class PairWithLink(val link: String) : UiEvent
    data object CloseScanner : UiEvent
    data object WordsMatch : UiEvent
    data object WordsDoNotMatch : UiEvent
    data object ConsentContinue : UiEvent
    data object ConsentLearnMore : UiEvent
    data object PairAgain : UiEvent
    data object SendWaitingFirst : UiEvent
    data object DiscardAndPair : UiEvent
    data object KeepThisPairing : UiEvent

    // --- photos and files ---------------------------------------------------------------------
    data object AttachMenu : UiEvent
    data class AttachPick(val source: String) : UiEvent
    data class AttachRemove(val id: String) : UiEvent
    data object SendAttachments : UiEvent
    data class UploadStop(val messageId: String) : UiEvent
    data class UploadRetry(val messageId: String) : UiEvent
    data class OpenReference(val messageId: String) : UiEvent
    data class OpenViewer(val messageId: String, val index: Int) : UiEvent
    data object CloseViewer : UiEvent
    data object ChooseAnotherFile : UiEvent
    data object AttachCardNotNow : UiEvent
    data object ShareSend : UiEvent
    data object ShareCancel : UiEvent
    data object ShareOpenToPair : UiEvent

    // --- settings, notifications, updates -----------------------------------------------------
    data object OpenSettings : UiEvent
    data object CloseOverlay : UiEvent
    data class Notifications(val on: Boolean) : UiEvent
    data class Previews(val on: Boolean) : UiEvent
    data object NotificationsNotNow : UiEvent
    data object OpenSystemSettings : UiEvent
    data object CheckForUpdates : UiEvent
    data object UpdateInStore : UiEvent
    data object UpdateLater : UiEvent
    data object Support : UiEvent
    data object PrivacyPolicy : UiEvent
    data object WhereMessagesGo : UiEvent
    data object ForgetPairing : UiEvent
    data object ForgetConfirmed : UiEvent
    /** The refused forget's way out: discard what waits, then forget (core `DiscardAndForget`). */
    data object DiscardAndForget : UiEvent
    data object ShowWaiting : UiEvent
    /** The microphone-off card's "Not now". */
    data object MicrophoneNotNow : UiEvent
    /** The microphone-off card's "Allow microphone", offered only while the system would still ask. */
    data object MicrophoneAskAgain : UiEvent
}

/** The core action for [this], or null when core has none yet. */
fun UiEvent.toAction(): Action? = when (this) {
    is UiEvent.Draft -> Action.Compose(text)
    is UiEvent.Reading -> Action.RememberReading(anchor)
    is UiEvent.AnswerQuestion -> Action.AnswerQuestion(id, options, text, revision)
    UiEvent.SendText -> Action.Send
    UiEvent.TryNow -> Action.Retry
    is UiEvent.Discard -> Action.Discard(clientId)
    is UiEvent.PairWithLink -> Action.Pair(link)
    UiEvent.WordsMatch -> Action.ConfirmWords(true)
    // The dialog about unsent work in the way of pairing: send what waits (core's retry), or keep
    // things as they are (closing clears core's refusal, and the dialog with it).
    UiEvent.SendWaitingFirst -> Action.Retry
    UiEvent.KeepThisPairing -> Action.CloseSheet
    UiEvent.WordsDoNotMatch -> Action.ConfirmWords(false)
    // Voice (core `Voice.kt`, the round-12 thresholds).
    is UiEvent.VoiceDown -> Action.VoicePress(id, widthDp.toDouble(), at)
    is UiEvent.VoiceMove -> Action.VoiceMove(dxDp.toDouble(), dyDp.toDouble(), at)
    is UiEvent.VoiceUp -> Action.VoiceRelease(at)
    is UiEvent.VoiceTouchCanceled -> Action.VoiceTouchCanceled(at)
    is UiEvent.VoiceCancelTapped -> Action.VoiceLockedCancel(at)
    is UiEvent.VoiceSendTapped -> Action.VoiceLockedSend(at)
    is UiEvent.VoiceStartHandsFree -> Action.VoiceStartLocked(id, widthDp.toDouble(), at)
    UiEvent.VoiceSettled -> Action.VoiceSettled
    is UiEvent.RecordingSend -> Action.SendKept(id)
    is UiEvent.RecordingDiscard -> Action.DiscardKept(id)
    UiEvent.RecordingPlay -> Action.PlayKept
    // "Hear it" and its Stop under Rich's reply (core `ReplyAudio.kt`, `GET /api/audio`).
    is UiEvent.HearReply -> Action.HearReply(messageId)
    is UiEvent.StopReply -> Action.StopReply(messageId)
    // A file Rich attached (core `RichFiles.kt`, `GET /api/messages?kind=file`).
    is UiEvent.OpenRichFile -> Action.OpenRichFile(fileId)
    is UiEvent.StopRichFile -> Action.StopRichFile(fileId)
    is UiEvent.CloseRichFile -> Action.RichFileViewed(fileId)
    UiEvent.NearOldest -> Action.LoadOlder
    // Settings, notifications, updates (core `Settings.kt`).
    UiEvent.OpenSettings -> Action.OpenSheet(Sheet.SETTINGS)
    UiEvent.CloseOverlay -> Action.CloseSheet
    UiEvent.ShowWaiting -> Action.CloseSheet
    UiEvent.WhereMessagesGo -> Action.OpenSheet(Sheet.WHERE_MESSAGES_GO)
    // Settings' "Where your messages go" is the consent screen again: its Continue is the way back out.
    UiEvent.ConsentContinue -> Action.CloseSheet
    UiEvent.UsePairingLink -> Action.OpenSheet(Sheet.PAIRING_LINK)
    UiEvent.ForgetPairing -> Action.ForgetPairing
    UiEvent.ForgetConfirmed -> Action.ConfirmForget
    UiEvent.DiscardAndForget -> Action.DiscardAndForget
    UiEvent.OpenSystemSettings -> Action.OpenSystemSettings
    // The microphone-off card (core `Voice.kt`, D03).
    UiEvent.MicrophoneAskAgain -> Action.AskMicrophone
    UiEvent.MicrophoneNotNow -> Action.DismissMicrophoneCard
    is UiEvent.Notifications -> if (on) Action.TurnOnNotifications else Action.TurnOffNotifications
    is UiEvent.Previews -> Action.SetPreviews(on)
    UiEvent.NotificationsNotNow -> Action.DismissNotificationOffer
    UiEvent.UpdateInStore -> Action.OpenAppStore
    UiEvent.UpdateLater -> Action.DismissUpdate
    UiEvent.Support -> Action.OpenSupport
    UiEvent.PrivacyPolicy -> Action.OpenPrivacyPolicy
    // Photos and files (core `Attach.kt`): the + menu's three sources, the tray, the cards.
    is UiEvent.AttachPick -> AttachSource.entries.firstOrNull { it.name.equals(source, ignoreCase = true) }?.let { Action.PickAttachments(it) }
    is UiEvent.AttachRemove -> Action.RemoveAttachment(id)
    UiEvent.SendAttachments -> Action.Send
    UiEvent.ChooseAnotherFile -> Action.PickAttachments(AttachSource.FILES)
    UiEvent.AttachCardNotNow -> Action.DismissAttachNotice
    // "Try again" on a photo or file message: the outbox's retry, which resumes every waiting message.
    is UiEvent.UploadRetry -> Action.Retry
    // On Android updates come from Google Play: the Settings row and the dialog's "Check again" open the listing.
    UiEvent.CheckForUpdates -> Action.CheckForUpdates
    // The consent screen's "Learn more" is the whole policy (privacy evidence E1).
    UiEvent.ConsentLearnMore -> Action.OpenPrivacyPolicy
    else -> null
}

/** Screen events that need a core action before they do anything (reported to stream A1). */
val NOT_YET_IN_CORE: List<String> = listOf(
    "(built: the voice gesture, kept recordings, notifications, settings sheet, updates — core 5351078e, c7207408)",
    "play a remote voice message or reply (platform audio)",
    "play a voice message",
    "(built: hear / stop a reply — core ReplyAudio.kt)",
    "pairing surfaces core does not model: consent",
    "(built: pair with a link, words match / do not match, forget — core c2a1a20a; the scanner, the camera, the link sheet, pair again and the blocked-by-unsent choices — ui/pairing)",
    "show the waiting messages (from the forget refusal)",
    "(built: photos and files — the + menu's Photos, Camera and Files, the tray, the Mac's limits, the cards, retry — core Attach.kt)",
    "stop an upload mid-way (core has no byte progress or cancel yet)",
)
