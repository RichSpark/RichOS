package dev.richos.android.ui.model

import androidx.compose.runtime.Immutable
import dev.richos.android.core.AppState
import dev.richos.android.core.AttachNotice as CoreAttachNotice
import dev.richos.android.core.AttachmentDescription
import dev.richos.android.core.isPhoto
import dev.richos.android.core.ConnectionReason
import dev.richos.android.core.Microphone
import dev.richos.android.core.NotificationStatus as CoreNotifications
import dev.richos.android.core.Sheet
import dev.richos.android.core.Toast
import dev.richos.android.core.UpdateNotice as CoreUpdate
import dev.richos.android.core.VoiceEnding
import dev.richos.android.core.VoicePhase as CorePhase
import dev.richos.android.core.KeptReason as CoreKept
import dev.richos.android.core.Echo
import dev.richos.android.core.Line
import dev.richos.android.core.localQuestionAnswer
import dev.richos.android.core.OutboxItem
import dev.richos.android.core.OutboxState
import dev.richos.android.core.SentMessage
import dev.richos.android.core.PairingPhase
import dev.richos.android.core.ReplyAudioRule
import dev.richos.android.core.RichCore
import dev.richos.android.core.FileFetch
import dev.richos.android.core.FileFetchState
import dev.richos.android.core.RichFile
import dev.richos.android.core.protocol.Row
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/**
 * Everything one frame of the app shows: PURE PRESENTATION.
 *
 * [app] is core's [AppState], and wherever it already knows a thing the screens read it from
 * there: the conversation (the Mac's rows: who spoke, text or voice, a reply still arriving, a reply
 * with audio), the draft, the theme, whether the phone is online, the pairing (intro, the code on its way,
 * the six words, a refusal, removal from the Mac), the outbox (every pending bubble, and whether
 * pairing or forgetting is blocked by unsent work, the "Waiting to send" card), what the Mac can
 * do (voice, photos and files), whether the gold circle is a microphone or a send arrow, and the
 * connection (the one nameplate line, only after core's 3 s of trouble; the composer off for a Mac
 * that needs a newer app; cached history before this launch reached the Mac). [refusal] is the
 * sentence core gave for the last refused action.
 *
 * Every other field is a STAND-IN for state core does not own yet (escalation
 * esc-20260922T204145Z-7e81fe5e; lead's answer: "pure presentation … as each lands, switch that
 * screen group to derive from core and delete the stand-in field"). A stand-in holds a value to
 * draw, never a rule: no thresholds, no timers that decide anything, no protocol. The voice
 * gesture's numbers, the outbox, pairing and connection logic belong to core, where the command
 * line proves them. Each stand-in names the core domain that replaces it.
 */
@Immutable
data class ScreenModel(
    val app: AppState,
    /** STAND-IN (core: outbox voice items carry no length). Milliseconds by outbox client id. */
    val voiceMs: Map<String, Long> = emptyMap(),
    /** STAND-IN (platform: audio playback). Hearing a reply: preparing or playing, by row id. */
    val replyAudio: Map<String, ReplyAudio> = emptyMap(),
    /** STAND-IN (platform: audio playback). The voice message playing and how far, 0..1. */
    val playing: Pair<String, Float>? = null,
    /** Where times are shown (the phone's zone; fixed in fixtures so frames never move). */
    val zone: ZoneId = ZoneId.systemDefault(),
    /** Optional catalog pose; production derives paging from the core. */
    val history: HistoryEdge? = null,
    /** Core's sentence for the last action it refused (`AppStore.lastRefusal`), or null. */
    val refusal: String? = null,
    /**
     * STAND-IN (platform: camera, first-run consent, stored-session version). A pairing surface
     * core does not model: the scanner, the camera being off, the consent screen, a saved session
     * from a newer app. Every other pairing screen derives from [AppState.pairing].
     */
    val pairingSurface: PairingSurface? = null,
    /**
     * PLATFORM (the camera behind the scanner): still opening, showing, or not there at all. Review
     * frames default to [ScannerCamera.READY] and draw round 12's pretend Mac in place of the feed.
     */
    val scannerCamera: ScannerCamera = ScannerCamera.READY,
    /**
     * REVIEW POSE, not state: how far into a transition (the bin, the lock, the send) a frame is
     * drawn, in ms. Null at run time: the screen then animates it from 0 when core's recording
     * ends, and tells core when the transition has played (`voice-settled`).
     */
    val voiceMomentMs: Int? = null,
    /** REVIEW POSE: the microphone level and halo wobble a frame is drawn at (the live app reads the microphone). */
    val voiceLevel: Float? = null,
    /** STAND-IN (core: composer limits, attachments). A one-line notice core has no toast for yet. */
    val localNotice: InlineNotice? = null,
    /** What the Settings sheet reports beyond core's notifications, read from core unless a frame poses it. */
    val settings: SettingsInfo = SettingsInfo(availableVersion = app.update?.version),
    /** Review frames only: Android's microphone prompt drawn over the press. */
    val overlay: Overlay? = null,
    /** STAND-IN (core: notifications). A reply notification as the system shade shows it (review frame). */
    val shade: ShadeNotification? = null,
    /** The tray and the photos-and-files cards (core `pendingAttachments`, `attachNotice`), unless a frame poses them. */
    val attach: AttachState = LiveAttachments.state(app),
    /** STAND-IN (core: attachments). Attachment messages and the replies quoting them, shown after [extraAfter]. */
    val extra: List<Message> = emptyList(),
    /** The row id the [extra] messages follow; null puts them after the last row. */
    val extraAfter: String? = null,
    /** Navigation: the photo or file opened full screen. */
    val viewer: Viewer? = null,
    /** STAND-IN (platform: share target). Share to Rich, over the app the user shared from. */
    val share: ShareSheet? = null,
    /** Review frames only: the system surface the attach flow has handed to. */
    val picker: SystemPicker? = null,
    /** Review frames only: draw a keyboard so the composer's ride above it can be judged. */
    val keyboardDrawn: Boolean = false,
    /** Initial scroll pose. At runtime the reader owns the scroll; this only poses a frame. */
    val scroll: ScrollPose = ScrollPose.FOLLOWING,
    /** Presentation clock for "Now" labels on pending messages: epoch ms, or null. */
    val nowMs: Long? = null,
) {
    val historyEdge: HistoryEdge get() = history ?: when {
        app.loadingOlder -> HistoryEdge.LOADING_OLDER
        app.olderAvailable -> HistoryEdge.MORE_AVAILABLE
        else -> HistoryEdge.BEGINNING
    }
    /** The reply opened from a notification (core `focusMessageId`), which glows once. */
    val focusedId: String? get() = app.focusMessageId

    /** Voice switched off by policy while a problem is fixed (core `voicePaused`). */
    val voicePaused: Boolean get() = app.voicePaused

    /** The update notice at its prominence (core `update`). */
    val update: UpdateNotice?
        get() = app.update?.let { u ->
            when (u.prominence) {
                CoreUpdate.Prominence.BANNER -> UpdateNotice.Banner(u.version, u.message)
                CoreUpdate.Prominence.DIALOG -> UpdateNotice.Dialog(u.version, u.message)
                CoreUpdate.Prominence.REQUIRED -> UpdateNotice.Required(u.version)
            }
        }

    /** Which sheet or dialog core has open. */
    val sheet: Sheet? get() = app.sheet

    /** The notifications row as the Settings sheet says it (core `notifications`). */
    val notificationStatus: NotificationStatus
        get() = when (app.notifications.status) {
            CoreNotifications.ON -> NotificationStatus.ON
            CoreNotifications.OFF, CoreNotifications.NOT_ASKED -> NotificationStatus.OFF
            CoreNotifications.TURNING_ON -> NotificationStatus.TURNING_ON
            CoreNotifications.DENIED -> NotificationStatus.DENIED
            CoreNotifications.UNSUPPORTED -> NotificationStatus.UNSUPPORTED
            CoreNotifications.PLATFORM_UNAVAILABLE -> NotificationStatus.PROVIDER_UNAVAILABLE
            CoreNotifications.SERVICE_UNAVAILABLE -> NotificationStatus.SERVICE_UNAVAILABLE
        }

    /** The notification offer: asked once, until answered or "Not now" (core `offerDismissed`). */
    val notificationOffer: Boolean
        get() = app.notifications.status == CoreNotifications.NOT_ASKED && !app.notifications.offerDismissed && app.pairing.phase == PairingPhase.PAIRED

    /**
     * The recording as the screen draws it, from core's `voice`: the phase, the finger, the
     * progress toward cancel and lock, the elapsed time; an ending plays its transition (the bin
     * for a cancel, the locked cancel, the send) at [voiceMomentMs].
     */
    val voice: VoicePose?
        get() {
            val v = app.voice ?: return null
            val level = voiceLevel ?: v.levels.lastOrNull()?.toFloat() ?: 0.4f
            val base = VoicePose(
                phase = when (v.phase) {
                    CorePhase.PRESSED -> VoicePhase.PRESSED
                    CorePhase.HELD -> VoicePhase.HELD
                    CorePhase.LOCKED -> VoicePhase.LOCKED
                    CorePhase.ENDING -> if (v.wasLocked) VoicePhase.LOCKED else VoicePhase.HELD
                },
                elapsedMs = app.voiceElapsedMs ?: v.elapsedMs,
                level = level,
                dxDp = v.dx.toFloat(),
                dyDp = v.dy.toFloat(),
                cancelProgress = v.cancelProgress.toFloat(),
                lockProgress = v.lockProgress.toFloat(),
            )
            // The lock transition: the screen plays it when core's phase turns locked (a review
            // frame poses it with [voiceMomentMs]).
            if (v.phase == CorePhase.LOCKED && voiceMomentMs != null) return base.copy(moment = VoiceMoment.LOCKING, momentMs = voiceMomentMs)
            if (v.phase != CorePhase.ENDING) return base
            return when (v.ending) {
                VoiceEnding.SENT -> base.copy(moment = VoiceMoment.SEND, momentMs = voiceMomentMs ?: 0)
                VoiceEnding.CANCELED -> base.copy(moment = if (v.wasLocked) VoiceMoment.LOCKED_CANCEL else VoiceMoment.BIN, momentMs = voiceMomentMs ?: 0)
                // Too short: the button springs back and the line explains (the toast); the ceiling: the card.
                else -> null
            }
        }

    /** The recording kept on the phone, unsent (core `keptRecordings`; the newest shows). */
    val keptRecording: KeptRecording?
        get() = app.keptRecordings.lastOrNull()?.let { k ->
            KeptRecording(
                durationMs = k.durationMs,
                reason = when (k.reason) {
                    CoreKept.CEILING -> KeptReason.CEILING
                    CoreKept.INTERRUPTED -> KeptReason.INTERRUPTED
                    else -> KeptReason.KEPT
                },
                id = k.id,
                playing = app.playingRecordingId == k.id,
            )
        }

    /** The one-line notice above the composer: core's toast, else a local one. */
    val inlineNotice: InlineNotice?
        get() = when (app.toast) {
            Toast.TOO_SHORT -> InlineNotice.TooShort
            Toast.CEILING_WARNING -> InlineNotice.CeilingWarning
            else -> localNotice ?: (app.attachNotice as? CoreAttachNotice.Limit)?.let { InlineNotice.AttachLimit(it.max) }
        }

    /**
     * The cards core raised above the composer. The microphone-off card (`rec-mic-denied`, D03)
     * follows core's `microphoneCard`, which only a press while the microphone is off sets; its
     * action is the system's question while Android would still show it, else Settings.
     */
    val cards: List<ComposerCard>
        get() = if (app.microphoneCard && app.microphone == Microphone.DENIED) listOf(ComposerCard.MicrophoneOff(canAsk = app.microphoneCanAsk)) else emptyList()

    /** Android's microphone prompt is up (core `microphonePrompt`). */
    val microphonePrompt: Boolean get() = app.microphonePrompt || overlay == Overlay.MicrophonePermission

    /** The pairing screen to draw, from core's pairing phase and problem (or a platform surface). */
    val pairingStep: PairingStep?
        get() {
            pairingSurface?.let { return it.step }
            val p = app.pairing
            return when (p.phase) {
                PairingPhase.UNPAIRED -> when (p.problem) {
                    null -> PairingStep.INTRO
                    PROBLEM_REVOKED -> null
                    PROBLEM_REFUSED -> PairingStep.REFUSED
                    else -> PairingStep.PROBLEM
                }
                PairingPhase.EXCHANGING -> PairingStep.IN_PROGRESS
                PairingPhase.CONFIRMING -> PairingStep.WORDS
                PairingPhase.AWAITING_MAC -> PairingStep.WAITING_FOR_MAC
                PairingPhase.PAIRED -> null
            }
        }

    /** The one terminal takeover: the Mac forgot this phone (core: unpaired with problem `revoked`, or the link said revoked). */
    val removedFromMac: Boolean
        get() = (app.pairing.phase == PairingPhase.UNPAIRED && app.pairing.problem == PROBLEM_REVOKED) ||
            app.connection.reason == ConnectionReason.REVOKED

    /** History shown from this phone before this launch has reached the Mac, while there is trouble. */
    val cachedWhileOffline: Boolean
        get() = !app.connection.hasConnected && app.connection.notice != null && app.messages.isNotEmpty()

    /** Pairing was refused because a message still waits: the dialog with its way out. */
    val pairingBlocked: Int? get() = if (refusal == RichCore.UNSENT_BEFORE_PAIRING && app.outbox.isNotEmpty()) app.outbox.size else null

    /** Forgetting was refused because messages still wait (core's forget-blocked sheet, or its refusal). */
    val forgetBlocked: Int?
        get() = if ((app.sheet == Sheet.FORGET_BLOCKED || refusal == RichCore.UNSENT_BEFORE_FORGET) && app.outbox.isNotEmpty()) app.outbox.size else null

    /** The Mac's per-file limit in whole MB (core `attachmentLimits`, from its hello; 25 MiB when it has not said). */
    val attachLimitMb: Int get() = ((app.attachmentLimits?.maxFileBytes ?: 26_214_400L) / 1_048_576L).toInt()

    /** How many photos and files one message may carry (core `attachmentLimits`). */
    val attachMaxItems: Int get() = app.attachmentLimits?.maxFilesPerMessage ?: 10

    /** The Mac takes photos and files: its hello listed "attachments" (`phone/attachments.rs`), or it has not said yet. */
    val attachmentsSupported: Boolean get() = app.capabilities.isEmpty() || "attachments" in app.capabilities

    /** The Mac takes voice: its last hello listed "voice" (contract §5.4), or it has not said yet. */
    val voiceSupported: Boolean get() = app.capabilities.isEmpty() || "voice" in app.capabilities

    /**
     * The nameplate line: core's connection notice (null until 3 s of trouble, at once for offline,
     * service, revoked and incompatible); else voice paused by policy; else, when a kept recording
     * or a tapped + cannot go because the Mac does not take it, the line that says so.
     */
    val notice: ConnectionNotice
        get() = when (app.connection.notice) {
            ConnectionReason.RECONNECTING -> ConnectionNotice.RECONNECTING
            ConnectionReason.PHONE_OFFLINE -> ConnectionNotice.OFFLINE
            ConnectionReason.SERVICE_UNAVAILABLE -> ConnectionNotice.SERVICE_UNAVAILABLE
            ConnectionReason.MAC_UNREACHABLE -> ConnectionNotice.MAC_UNREACHABLE
            ConnectionReason.TAILSCALE_OFF -> ConnectionNotice.TAILSCALE_OFF
            ConnectionReason.INCOMPATIBLE -> ConnectionNotice.MAC_NEEDS_UPDATE
            else -> nameplateLocal
        }

    private val nameplateLocal: ConnectionNotice
        get() = when {
            voicePaused -> ConnectionNotice.VOICE_PAUSED
            keptRecording != null && !voiceSupported -> ConnectionNotice.VOICE_UNSUPPORTED
            attach.macOffCard && !attachmentsSupported -> ConnectionNotice.ATTACHMENTS_UNSUPPORTED
            else -> ConnectionNotice.NONE
        }

    /**
     * The "Waiting to send" card: core's last send pass left messages waiting for a retryable
     * reason (the Mac out of reach, a fault, too many requests). Its count is core's; the card
     * offers Try now, which is core's `retry`.
     */
    val waitingToSend: Int?
        get() {
            val report = app.lastSend ?: return null
            val waiting = app.outbox.count { it.state == OutboxState.WAITING }
            return if (report.reason != null && report.reason in RETRYABLE && waiting > 0) waiting else null
        }

    /** Why the composer takes nothing, or null. Derived from the notice, never decided here. */
    val composerDisabledReason: String? get() = if (notice == ConnectionNotice.MAC_NEEDS_UPDATE) "Sending is off until your Mac updates" else null

    /** A press may record (core `canRecord` once paired; the policy and the Mac's voice otherwise). */
    val voiceAvailable: Boolean
        get() = (if (app.pairing.phase == PairingPhase.PAIRED) app.canRecord || app.voice != null else voiceSupported) &&
            !voicePaused && composerDisabledReason == null

    /**
     * The thread as drawn: core's transcript ([AppState.transcript]): the Mac's rows, then your
     * messages the Mac accepted and has not echoed yet, then your unsent ones. One of your
     * messages keeps one list identity ([Message.key]) from Send to the Mac's echo, so it is one
     * row that changes state in place, never a row that goes and another that comes (D01).
     */
    val thread: List<Message>
        get() {
            val lines = app.transcript
            val rows = lines.filterIsInstance<Line.Mac>().flatMap { bubbles(it.row, it.clientId, it.echo) }
            val mine = lines.flatMap { line ->
                when (line) {
                    is Line.Accepted -> acceptedBubbles(line.message)
                    is Line.Pending -> if (line.item.kind in listOf("answer", "question_seen")) emptyList() else pendingBubbles(line.item)
                    is Line.Mac -> emptyList()
                }
            }
            val folded = withFilesUnderWords(rows)
            if (extra.isEmpty()) return folded + mine
            val at = extraAfter?.let { id -> folded.indexOfFirst { it.id == id } + 1 }?.takeIf { it > 0 } ?: folded.size
            return folded.take(at) + extra + folded.drop(at) + mine
        }

    /**
     * The files Rich attached sit in his bubble, under his words (round 36 "Phone: in the chat"):
     * the Mac sends each as its own row right after his reply (`phone/rows.rs`), and a file row
     * joins the finished words of Rich's just before it. One with no words of his before it (a file
     * for a job) is a bubble of its own.
     */
    private fun withFilesUnderWords(rows: List<Message>): List<Message> {
        val out = ArrayList<Message>(rows.size)
        for (m in rows) {
            val prev = out.lastOrNull()
            val fileOnly = m.files.isNotEmpty() && (m.body as? Body.Text)?.text?.isEmpty() == true
            if (fileOnly && prev != null && prev.speaker == Speaker.RICH && prev.body is Body.Text &&
                !prev.replying && !prev.streaming && !prev.unfinished
            ) {
                out[out.lastIndex] = prev.copy(files = prev.files + m.files)
            } else {
                out += m
            }
        }
        return out
    }

    /**
     * The file Rich attached whose viewer is open (round 36 "Phone: opened"): core says it is kept
     * on this phone and was tapped. Its row gives the time the viewer names.
     */
    val fileViewer: Triple<RichFile, FileFetch, String>?
        get() {
            val fetch = app.fileFetches.values.firstOrNull { it.state == FileFetchState.KEPT && it.open && it.path != null } ?: return null
            val row = app.messages.lastOrNull { it.file?.id == fetch.id } ?: return null
            return Triple(row.file!!, fetch, TimeLabels.row(row.createdAt, nowMs, zone))
        }

    /** A voice message's length on the phone: the frame's pose, else the recording's own. */
    private fun voiceLength(clientId: String, seconds: Double?): Long = voiceMs[clientId] ?: seconds?.let { (it * 1000).toLong() } ?: 0L

    /**
     * One of the Mac's rows as bubbles. Your photos and files come back as the words the Mac gave
     * Rich (`AttachmentDescription`): drawn as round 12.1's album and file bubbles, the photos as
     * plain tiles (their pixels are on the Mac, not here), never as that text. [clientId] is set
     * when the row is the echo of one of your messages: the bubbles keep that message's identity.
     */
    private fun bubbles(row: Row, clientId: String? = null, echo: Echo? = null): List<Message> {
        val parsed = if (row.role == "ceo") AttachmentDescription.parse(row.text) else null
        if (parsed == null) return listOf(bubble(row, clientId, echo))
        val base = bubble(row, clientId, echo)
        // Your photos keep the sizes they had on the phone, so the album does not reflow at the echo.
        val own = echo?.attachments.orEmpty().filter { it.isPhoto }
        val photos = parsed.files.filter { it.isPhoto }.mapIndexed { i, f ->
            Photo(LiveAttachments.ON_MAC + (clientId ?: row.id) + "#" + i, own.getOrNull(i)?.width ?: 4, own.getOrNull(i)?.height ?: 3, f.name, f.size)
        }
        val files = parsed.files.filterNot { it.isPhoto }.map { LiveAttachments.fileInfo(it.name, it.size) }
        return LiveAttachments.split(photos, files, parsed.caption).mapIndexed { i, body ->
            base.copy(id = if (i == 0) row.id else "${row.id}#$i", key = if (i == 0) base.key else "${base.key}#$i", body = body)
        }
    }

    /** One of the Mac's rows as a bubble (contract §5.4: `role` ceo|rich, `kind` text|voice, `state`). */
    private fun bubble(row: Row, clientId: String? = null, echo: Echo? = null): Message {
        // A file Rich attached (round 36 screen 3): no words of its own, the card under his.
        row.file?.takeIf { row.kind == "file" && row.role != "ceo" }?.let { file ->
            return Message(
                id = row.id,
                speaker = Speaker.RICH,
                body = Body.Text(""),
                time = TimeLabels.row(row.createdAt, nowMs, zone),
                files = listOf(RichFileCard(file, app.fileFetches[file.id])),
            )
        }
        val question = row.question?.takeIf { "questions" in app.capabilities }
        val rich = row.role != "ceo"
        val arriving = rich && !row.complete
        // Your voice message comes back as the words the Mac heard: it stays the voice message you
        // sent, with its own length and waveform (the iPhone's rule, `ceffcd9a`).
        val voice = row.kind == "voice" || echo?.kind == "voice"
        val seed = (clientId ?: row.id).hashCode()
        return Message(
            id = row.id,
            key = clientId ?: row.id,
            speaker = if (rich) Speaker.RICH else Speaker.ME,
            // A voice row's length is core's (`duration_ms`); absent, the bubble shows no length, never 0:00.
            body = if (question != null) app.outbox.firstOrNull { it.questionId == question.id }.let { pending -> Body.Question(question.copy(answer = pending?.localQuestionAnswer() ?: question.answer), pending?.let { if (it.state == OutboxState.BLOCKED) it.lastReason ?: "Your answer needs attention" else if (app.online) "On its way to Rich" else "Waiting to send · your Mac is out of reach" }, pending?.text, pending?.let { it.state == OutboxState.WAITING && it.attempts == 0 && !app.online } == true) } else if (voice) Body.Voice(row.durationMs ?: clientId?.let { voiceLength(it, echo?.seconds) }?.takeIf { it > 0 } ?: voiceMs[row.id] ?: 0L, Waves.forSeed(seed, 42))
                else Body.Text(row.text),
            time = TimeLabels.row(row.createdAt, nowMs, zone),
            // "Arriving now" (the dots, the caret) needs an open stream behind it. Without one (a
            // cold launch, the link lost) the reply is unfinished and still: D02 (native acceptance
            // r1) drew a restored reply's caret at 2 Hz for as long as the Mac was away.
            replying = arriving && app.online && row.text.isEmpty(),
            streaming = arriving && app.online && row.text.isNotEmpty(),
            unfinished = arriving && !app.online,
            // "Hear it" by the reference client's rule (`has_audio`, or the Mac offers `audio`); the
            // reply being heard is core's, preparing then playing.
            audio = replyAudio[row.id]
                ?: app.replyPlayback?.takeIf { it.id == row.id }?.let { if (it.playing) ReplyAudio.PLAYING else ReplyAudio.PREPARING }
                ?: if (ReplyAudioRule.hearable(row, app.capabilities)) ReplyAudio.READY else ReplyAudio.NONE,
            playProgress = playing?.takeIf { it.first == row.id }?.second ?: 0f,
            focused = row.id == focusedId,
        )
    }

    /**
     * A photos-and-files message waiting in the outbox as round 12.1 draws it: the album (the phone
     * still has the pixels) or the file, with the clock while it waits and Try again when it needs
     * attention. While it is being sent it reads "Sending…" like any message: core reports no byte
     * progress, so no ring pretends to fill.
     */
    private fun pendingBubbles(item: OutboxItem): List<Message> {
        val base = pendingBubble(item)
        if (item.kind != "attachments") return listOf(base)
        val all = item.attachments.orEmpty()
        val photos = all.filter { it.isPhoto }.map { LiveAttachments.photo(it) }
        val files = all.filterNot { it.isPhoto }.map { LiveAttachments.fileInfo(it.name, it.size) }
        val upload = when (item.state) {
            OutboxState.WAITING -> UploadStatus.QUEUED
            OutboxState.SENDING -> null
            OutboxState.BLOCKED -> UploadStatus.ATTENTION
        }
        return LiveAttachments.split(photos, files, item.text).mapIndexed { i, body ->
            val id = if (i == 0) item.clientId else "${item.clientId}#$i"
            base.copy(id = id, key = id, body = body, upload = upload)
        }
    }

    /**
     * One of your messages the Mac has accepted and not yet echoed: sent (the check mark), where
     * it was while pending. Photos are the Mac's now (the phone's staged copies are released on
     * acceptance), so they are drawn as the Mac's plain tiles, as the echo will draw them.
     */
    private fun acceptedBubbles(m: SentMessage): List<Message> {
        val base = Message(
            id = m.clientId,
            speaker = Speaker.ME,
            body = if (m.kind == "voice") Body.Voice(voiceLength(m.clientId, m.seconds), Waves.forSeed(m.clientId.hashCode(), 42)) else Body.Text(m.text),
            time = TimeLabels.pending(m.queuedAt, nowMs, zone),
        )
        if (m.kind != "attachments") return listOf(base)
        val all = m.attachments.orEmpty()
        val photos = all.filter { it.isPhoto }.mapIndexed { i, a -> Photo(LiveAttachments.ON_MAC + m.clientId + "#" + i, a.width ?: 4, a.height ?: 3, a.name, a.size) }
        val files = all.filterNot { it.isPhoto }.map { LiveAttachments.fileInfo(it.name, it.size) }
        return LiveAttachments.split(photos, files, m.text).mapIndexed { i, body ->
            val id = if (i == 0) m.clientId else "${m.clientId}#$i"
            base.copy(id = id, key = id, body = body)
        }
    }

    private fun pendingBubble(item: OutboxItem): Message = Message(
        id = item.clientId,
        speaker = Speaker.ME,
        body = if (item.kind == "voice") {
            Body.Voice(voiceLength(item.clientId, item.seconds), Waves.forSeed(item.clientId.hashCode(), 42))
        } else {
            Body.Text(item.text)
        },
        time = TimeLabels.pending(item.queuedAt, nowMs, zone),
        delivery = when (item.state) {
            OutboxState.WAITING -> Delivery.WAITING
            OutboxState.SENDING -> Delivery.SENDING
            OutboxState.BLOCKED -> Delivery.ATTENTION
        },
        outboxClientId = item.clientId,
        refusal = if (item.state == OutboxState.BLOCKED) refusalSentence(item.lastReason) else null,
    )
}

/**
 * The Mac's own sentence for a refusal (HTTP 422 `reason`), or null. A refusal whose answer carried
 * no sentence keeps the phone's one-word classification ("refused", "fault", "too large", a deferral
 * such as "link-lost"), which is not for a person; the Mac's sentences start with a capital and are
 * more than one word. iPhone parity: isaac-opus-r3floor1 `7faec1fda`.
 */
fun refusalSentence(reason: String?): String? {
    val text = reason?.trim() ?: return null
    return if (text.firstOrNull()?.isUpperCase() == true && text.contains(' ')) text else null
}

enum class Speaker { RICH, ME }

@Immutable
sealed interface Body {
    data class Question(val card: dev.richos.android.core.QuestionCard, val pending: String? = null, val savedAnswer: String? = null, val canEditLocal: Boolean = false) : Body
    data class Text(val text: String) : Body
    /** [wave] is the recording's own level, resampled to bars in 0..1 (NOTES "What an engineer should know"). */
    data class Voice(val durationMs: Long, val wave: List<Float>) : Body
    /** Photos sent as one album, with the caption riding on it. */
    data class Album(val photos: List<Photo>, val caption: String = "") : Body
    /** A file, with the caption when it is the batch's last. */
    data class File(val file: FileInfo, val caption: String = "") : Body
}

enum class Delivery { SENT, SENDING, WAITING, ATTENTION }

enum class ReplyAudio { NONE, READY, PREPARING, PLAYING }

@Immutable
data class Message(
    val id: String,
    val speaker: Speaker,
    val body: Body,
    val time: String,
    /**
     * The bubble's identity in the list. Your message keeps its client id from Send through the
     * Mac's echo, so the row changes state in place instead of leaving and arriving again.
     */
    val key: String = id,
    val delivery: Delivery = Delivery.SENT,
    /** Rich has started a reply and no words have arrived: three breathing dots. */
    val replying: Boolean = false,
    /** Words are arriving: the text so far with a gold caret. */
    val streaming: Boolean = false,
    /** A reply that stopped arriving with no stream open: the words so far, an ellipsis, no time, no motion. */
    val unfinished: Boolean = false,
    val audio: ReplyAudio = ReplyAudio.NONE,
    /** 0..1 of a playing voice message or reply. */
    val playProgress: Float = 0f,
    /** Opened from a notification: glows once in gold. */
    val focused: Boolean = false,
    /** Set for a bubble drawn from core's outbox, so Discard names the item. */
    val outboxClientId: String? = null,
    /** An attachment message's upload: its state and how many bytes have gone, 0..1. */
    val upload: UploadStatus? = null,
    val progress: Float = 0f,
    /** Rich's quote of what he is answering. */
    val ref: Reference? = null,
    /** "Shared from Photos": where a shared item came from. */
    val via: String? = null,
    /** Why the Mac refused this message, in its own words; null when it gave no sentence. */
    val refusal: String? = null,
    /** The files Rich attached, drawn under his words in this bubble (round 36 screen 3). */
    val files: List<RichFileCard> = emptyList(),
)

/** One file Rich attached, and where it stands on this phone (core `fileFetches`), or null if never tapped. */
@Immutable
data class RichFileCard(val file: RichFile, val fetch: FileFetch?)

enum class HistoryEdge { MORE_AVAILABLE, LOADING_OLDER, BEGINNING }

enum class ScrollPose { FOLLOWING, READING_OLDER, TOP }

enum class ConnectionNotice {
    NONE, RECONNECTING, OFFLINE, SERVICE_UNAVAILABLE, MAC_UNREACHABLE, MAC_NEEDS_UPDATE, VOICE_UNSUPPORTED, VOICE_PAUSED, ATTACHMENTS_UNSUPPORTED,
    TAILSCALE_OFF,
}

/** Every pairing screen. Most derive from core; see [PairingSurface] for the ones that do not. */
enum class PairingStep { INTRO, SCANNING, FOUND, CAMERA_DENIED, IN_PROGRESS, WORDS, WAITING_FOR_MAC, REFUSED, PROBLEM, NEEDS_NEWER_APP, CONSENT }

/** Pairing surfaces owned by the platform, not core: the camera, first-run consent, a newer session. */
enum class PairingSurface(val step: PairingStep) {
    SCANNING(PairingStep.SCANNING),
    FOUND(PairingStep.FOUND),
    CAMERA_DENIED(PairingStep.CAMERA_DENIED),
    CONSENT(PairingStep.CONSENT),
    NEEDS_NEWER_APP(PairingStep.NEEDS_NEWER_APP),
}

/** The camera behind the scanner (T3's four scanner states; "denied" is [PairingSurface.CAMERA_DENIED]). */
enum class ScannerCamera { CHECKING, READY, UNAVAILABLE }

/** Core's retryable send reasons (`TransportFailure.reason` with `retryable = true`). */
val RETRYABLE: Set<String> = setOf("unreachable", "fault", "rate-limited")

/** Core's pairing problem codes (`Pairing.problem`). */
const val PROBLEM_REFUSED = "refused"
const val PROBLEM_REVOKED = "revoked"

/** Where the gesture is. The phases and progress values are core's to compute; the screen draws them. */
enum class VoicePhase { PRESSED, HELD, LOCKED }

/** A transition being played, with [VoicePose.momentMs] into it. */
enum class VoiceMoment { NONE, LOCKING, BIN, LOCKED_CANCEL, SEND }

@Immutable
data class VoicePose(
    val phase: VoicePhase,
    val elapsedMs: Long = 0,
    /** Microphone level 0..1, already smoothed. */
    val level: Float = 0.4f,
    /** Finger travel from the touch-down point, in dp; negative is left / up. */
    val dxDp: Float = 0f,
    val dyDp: Float = 0f,
    /** 0..1 toward cancel, 0..1 toward lock, as core computes them. */
    val cancelProgress: Float = 0f,
    val lockProgress: Float = 0f,
    val moment: VoiceMoment = VoiceMoment.NONE,
    val momentMs: Int = 0,
    /** Wall clock in seconds for the halo's wobble; frames fix it for a stable screenshot. */
    val wobbleSeconds: Float = 0f,
)

enum class KeptReason { KEPT, CEILING, INTERRUPTED }

@Immutable
data class KeptRecording(val durationMs: Long, val reason: KeptReason = KeptReason.KEPT, val playing: Boolean = false, val id: String = "")

@Immutable
sealed interface InlineNotice {
    data class TooLong(val limit: Int) : InlineNotice
    data object TooShort : InlineNotice
    data object CeilingWarning : InlineNotice
    /** The tray is full: "Up to 10 at a time." [max] is the Mac's per-message limit. */
    data class AttachLimit(val max: Int) : InlineNotice
}

@Immutable
sealed interface ComposerCard {
    /** The microphone is off for RichConnect. [canAsk]: Android would still show its own question. */
    data class MicrophoneOff(val canAsk: Boolean) : ComposerCard
}

@Immutable
sealed interface UpdateNotice {
    data class Banner(val version: String, val line: String) : UpdateNotice
    data class Dialog(val version: String, val line: String) : UpdateNotice
    data class Required(val version: String) : UpdateNotice
}

enum class NotificationStatus { ON, OFF, TURNING_ON, DENIED, UNSUPPORTED, PROVIDER_UNAVAILABLE, SERVICE_UNAVAILABLE }

/**
 * The paired Mac, as a person reads it, while its own name is unknown: "Paired with your Mac",
 * "Rich on your Mac" (G1, UX audit richos-hq fffe1d1d §4.2; the iPhone's `mac?.name ?? "your Mac"`).
 * The Mac reports no name today: its pair answer has none (contract §2.3; the Mac's `/api/pair`
 * route), so every pairing reads this until it does. Never a sample name: that would be someone else's.
 */
const val YOUR_MAC = "your Mac"

@Immutable
data class SettingsInfo(
    /** The version the hosted update policy announced (core `update`), or null: nothing has said so. */
    val availableVersion: String? = null,
    val macName: String = YOUR_MAC,
)

@Immutable
sealed interface Overlay {
    data object MicrophonePermission : Overlay
}

@Immutable
data class ShadeNotification(val preview: String?)

/** Presentation formatting of times; no clock decides anything here. */
object TimeLabels {
    private val CLOCK: DateTimeFormatter = DateTimeFormatter.ofPattern("h:mm a", Locale.US)

    /** A pending message: "Now" for its first minute, then its time. */
    fun pending(queuedAtIso: String, nowMs: Long?, zone: ZoneId = ZoneId.systemDefault()): String {
        val at = runCatching { Instant.parse(queuedAtIso).toEpochMilli() }.getOrNull() ?: return ""
        if (nowMs == null || nowMs - at < 60_000) return "Now"
        return Instant.ofEpochMilli(at).atZone(zone).toLocalTime().format(CLOCK)
    }

    /** A row's time: "8:02 AM" today, "Yesterday 6:10 PM", else "Sep 20 6:10 PM" (round 12's forms). */
    fun row(createdAtIso: String?, nowMs: Long?, zone: ZoneId = ZoneId.systemDefault()): String {
        val at = createdAtIso?.let { runCatching { Instant.parse(it) }.getOrNull() } ?: return ""
        val local = at.atZone(zone)
        val clock = local.toLocalTime().format(CLOCK)
        val today = Instant.ofEpochMilli(nowMs ?: return clock).atZone(zone).toLocalDate()
        return when (local.toLocalDate()) {
            today -> clock
            today.minusDays(1) -> "Yesterday $clock"
            else -> local.format(DateTimeFormatter.ofPattern("MMM d", Locale.US)) + " " + clock
        }
    }

    /** `M:SS.t`, tenths ticking (round 12 `fmtTimer`). */
    fun timer(ms: Long): String {
        val t = ms / 100
        val tenth = t % 10
        val s = (t / 10) % 60
        val m = t / 600
        return "$m:${if (s < 10) "0" else ""}$s.$tenth"
    }

    /** `M:SS` for durations (round 12 `fmtDur`), rounded to the nearest second. */
    fun duration(ms: Long): String {
        val total = Math.round(ms / 1000.0)
        val m = total / 60
        val s = total % 60
        return "$m:${if (s < 10) "0" else ""}$s"
    }
}

/** Deterministic pretend waveforms for fixtures (round 12 `waveFor`): same seed, same bars. */
object Waves {
    fun forSeed(seed: Int, n: Int): List<Float> {
        var x = seed.toLong() and 0x7fffffff
        val out = ArrayList<Float>(n)
        for (i in 0 until n) {
            x = (x * 1103515245L + 12345L) and 0x7fffffffL
            val r = (x shr 8) / 8388608.0
            val v = 0.15 + kotlin.math.abs(kotlin.math.sin(i * 0.9 + seed)) * 0.6 * r + r * 0.25
            out.add(v.coerceIn(0.05, 1.0).toFloat())
        }
        return out
    }
}
