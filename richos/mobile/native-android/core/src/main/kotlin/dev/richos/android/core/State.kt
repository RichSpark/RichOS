package dev.richos.android.core

import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// The semantic state, field for field the shape the preserved phone core already prints
// (`richos/mobile/core/app.js` `state()` = session fields + `outbox`, `dueInMs`, `lastSend`),
// so a headless trace from this core and one from the JavaScript core read the same.
// Names are shared with the native iOS core (build plan §3.3: "the same type and action
// names so a reviewer can compare them side by side").

@Serializable
data class ConversationThread(val id: String, val title: String)

/** Dark "Sovereign" is what a new install opens in; light "Daybreak" is a choice (ceo-decisions §15). */
@Serializable
enum class Theme {
    @SerialName("dark") DARK,
    @SerialName("light") LIGHT,
}

/** Where pairing stands (phone protocol contract §2). */
@Serializable
enum class PairingPhase {
    /** No Mac. The pairing screens show. */
    @SerialName("unpaired") UNPAIRED,

    /** The code is on its way to the Mac (round-12 `pair-progress`). */
    @SerialName("exchanging") EXCHANGING,

    /** The Mac answered; the six words wait for the user's "They match" (round-12 `pair-words`). */
    @SerialName("confirming") CONFIRMING,

    /**
     * "They match" on the phone is sent; the Mac still waits for the person to press "They match"
     * ON THE MAC (pairing v2, Sage F1). The phone asks on [dev.richos.android.core.protocol.MacWait]'s
     * schedule, only while on screen, until the Mac answers, refuses, or the bound passes.
     */
    @SerialName("awaiting-mac") AWAITING_MAC,

    /** Confirmed. Messages may be sent. */
    @SerialName("paired") PAIRED,
}

/** Which of the two routes the pairing link chose (contract §1.1). Fixed for the whole pairing. */
@Serializable
enum class Route {
    @SerialName("tailnet") TAILNET,
    @SerialName("connect") CONNECT,
}

/**
 * The pairing, and what must survive a restart for it (contract §2.2 "What the phone must
 * persist"). [problem] names the last pairing outcome that was not a success, as a stable code a
 * screen chooses its words from: `refused` (a wrong or expired code, or the Mac already has a
 * phone), `rate-limited`, `unreachable`, `fault`, `revoked`; and pairing v2's three:
 * `mac-needs-update` (the Mac does not offer `pair-v2`), `mac-declined` (the Mac refused this
 * phone while it waited for the press on the Mac: "They do not match" there, or its window closed)
 * and `expired` (the press on the Mac did not come within the bound).
 *
 * The wait for the press on the Mac is persisted so a restart neither resets its request count
 * nor outlives its bound: [awaitingUntil] (epoch ms), [macAsks] (requests made) and [nextAskAt].
 */
@OptIn(ExperimentalSerializationApi::class)
@Serializable
data class Pairing(
    val phase: PairingPhase = PairingPhase.UNPAIRED,
    val apiBase: String? = null,
    val route: Route? = null,
    val deviceId: String? = null,
    val caFingerprint: String? = null,
    val words: List<String> = emptyList(),
    val challenge: String? = null,
    val problem: String? = null,
    /** While confirming: how long the Mac will wait for its own press, from its answer. */
    @EncodeDefault(EncodeDefault.Mode.NEVER) val macWaitBoundMs: Long? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val awaitingUntil: Long? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val macAsks: Int = 0,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val nextAskAt: Long? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val lastAskAt: Long? = null,
)

/** What the user owns on this phone and what survives a restart: `app.js`'s `model`, plus `theme` and `pairing`. */
@Serializable
data class Session(
    val threads: List<ConversationThread> = emptyList(),
    val selectedThreadId: String? = null,
    val draft: String = "",
    val online: Boolean = false,
    val paired: Boolean = false,
    val theme: Theme = Theme.DARK,
    val pairing: Pairing = Pairing(),
    /** The bounded message cache, per conversation, newest [CACHE_ROWS] rows (contract §2.2). */
    val cache: Map<String, List<dev.richos.android.core.protocol.Row>> = emptyMap(),
    /** What the Mac said it can do in its last `hello`; replaced, never merged (contract §5.4). */
    val capabilities: List<String> = emptyList(),
    val macBuild: String? = null,
    /** The OS's answer about the microphone, mirrored. */
    val microphone: Microphone = Microphone.UNKNOWN,
    /** Recordings kept on the phone and not sent (`rec-card`). */
    val keptRecordings: List<KeptRecording> = emptyList(),
    /** Written before capture starts; an abrupt process death recovers this file without sending. */
    val activeRecording: KeptRecording? = null,
    val notifications: Notifications = Notifications(),
    /** The hosted policy's last notice, kept so an offline launch still shows it. */
    val update: UpdateNotice? = null,
    /** Voice is switched off by policy (`upd-feature-off`); text still works. */
    val voicePaused: Boolean = false,
    /** The Mac's photo and file limits; null until a Mac that takes attachments says so. */
    val attachmentLimits: AttachmentLimits? = null,
    /** The Mac's push host id from a native registration, kept to validate incoming alerts. */
    val pushHostId: String? = null,
    /** Per conversation: whether the Mac holds rows older than the oldest one cached. */
    val olderAvailable: Map<String, Boolean> = emptyMap(),
    /**
     * The last live frame id the stream delivered: the Mac's HUB cursor, which a reconnect's
     * `since` counts in. Not a row's cursor: a phone message takes 3 live cursors but 2 history
     * positions (Echo's measurement), so the two drift apart. Null after a `hello`: a reconnect
     * then asks for a fresh `hello` rather than resuming from the `hello`'s id (RichCore `apply`).
     */
    val streamCursor: Long? = null,
    /** Photos and files chosen for the next message, staged on the phone, not yet sent. */
    val pendingAttachments: List<Attachment> = emptyList(),
    /** Durable send journal: consuming the composer and recording intent is one atomic write. */
    val pendingEnqueues: List<OutboxItem> = emptyList(),
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val readingAnchor: ReadingAnchor? = null,
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val completionReservations: List<Long> = emptyList(),
    /** Messages the Mac accepted and has not yet echoed ([Echoes]), in the order they were sent. */
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val sent: List<SentMessage> = emptyList(),
    /** Which of the Mac's cached rows are the echoes of this phone's messages ([Echoes]). */
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val echoes: List<Echo> = emptyList(),
) {
    companion object {
        const val CACHE_ROWS = 100

        /** Accepted-and-not-yet-echoed messages kept at most; the oldest goes first beyond it. */
        const val SENT_LIMIT = 100
    }
}

@Serializable
enum class OutboxState {
    @SerialName("waiting") WAITING,
    @SerialName("sending") SENDING,

    /** The Mac gave a final answer; this needs the user, not a retry (`queue.js` BLOCKED). */
    @SerialName("blocked") BLOCKED,
}

/**
 * One message the user has sent that the Mac has not yet acknowledged (`queue.js` item).
 * The last two fields exist only once set, as in `queue.js`, so they are omitted while null.
 */
@OptIn(ExperimentalSerializationApi::class)
@Serializable
data class OutboxItem(
    val clientId: String,
    val threadId: String,
    val kind: String,
    val text: String,
    val state: OutboxState = OutboxState.WAITING,
    val attempts: Int = 0,
    val queuedAt: String,
    val lastReason: String? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val notBefore: Long? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val resumedAfterInterruptedSend: Boolean? = null,
    /** Voice only, as `app.js` `send-voice` queues it: the recording, its codec, rate and length. */
    @EncodeDefault(EncodeDefault.Mode.NEVER) val fileId: String? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val codec: String? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val sampleRate: Int? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val seconds: Double? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val levels: List<Double>? = null,
    /** The exact request bytes (text, attachment commit) or signed path (voice), built once at enqueue. */
    @EncodeDefault(EncodeDefault.Mode.NEVER) val questionId: String? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val wire: String? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val attachments: List<Attachment>? = null,
    /**
     * The newest of the Mac's rows on screen when Send was pressed (its cursor and id): no row at
     * or before it can be this message's echo ([Echoes]). Absent on items queued by older builds.
     */
    @EncodeDefault(EncodeDefault.Mode.NEVER) val echoAfterCursor: Long? = null,
    @EncodeDefault(EncodeDefault.Mode.NEVER) val echoAfterMessageId: String? = null,
)

/** What one pass over the outbox did, as data (`queue.js` `flush` result). */
@Serializable
data class SendReport(
    val sent: Int = 0,
    val waiting: Int = 0,
    val blocked: Int = 0,
    val reason: String? = null,
    val duplicates: Int = 0,
    /** Something is queued and its retry clock has not come round yet. */
    val deferred: Int = 0,
)

@Serializable
data class AppState(
    val threads: List<ConversationThread>,
    val selectedThreadId: String?,
    val draft: String,
    val online: Boolean,
    val paired: Boolean,
    val theme: Theme,
    val pairing: Pairing,
    /** The selected conversation's rows, oldest first. */
    val messages: List<dev.richos.android.core.protocol.Row>,
    val capabilities: List<String>,
    val connection: ConnectionState,
    val outbox: List<OutboxItem>,
    val dueInMs: Long?,
    val lastSend: SendReport?,
    /** The recording in progress, if any (round-12 groups 4, 5, 6). */
    val voice: VoiceSession? = null,
    /** What the voice timer shows, in ms (`M:SS.t`); null when nothing records. */
    val voiceElapsedMs: Long? = null,
    val microphone: Microphone = Microphone.UNKNOWN,
    /** The system is asking for the microphone; the press that asked never records. */
    val microphonePrompt: Boolean = false,
    /** A press found the microphone off: the microphone-off card is up (`rec-mic-denied`, D03). */
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val microphoneCard: Boolean = false,
    /** While denied: the system would still show its question, so the card offers asking again, not Settings. */
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val microphoneCanAsk: Boolean = false,
    val keptRecordings: List<KeptRecording> = emptyList(),
    val toast: Toast? = null,
    /** A press may record: paired, the Mac offers voice, the Mac is compatible, voice not paused. */
    val canRecord: Boolean = false,
    val notifications: Notifications = Notifications(),
    val sheet: Sheet? = null,
    /** The reply opened from a notification, which glows once (`conv-focused`). */
    val focusMessageId: String? = null,
    val update: UpdateNotice? = null,
    val voicePaused: Boolean = false,
    val attachmentLimits: AttachmentLimits? = null,
    /** What sits above the oldest message: more to load (true), the beginning (false). */
    val olderAvailable: Boolean = false,
    /** A chunk of older messages is on its way (`conv-older-loading`). */
    val loadingOlder: Boolean = false,
    /** The last live frame id delivered (see [Session.streamCursor]); a reconnect replays from it. */
    val streamCursor: Long? = null,
    /** Photos and files waiting in the composer for the next send. */
    val pendingAttachments: List<Attachment> = emptyList(),
    /** A line or card above the composer about photos and files (transient). */
    val attachNotice: AttachNotice? = null,
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val playingRecordingId: String? = null,
    /** Rich's reply being heard ("Hear it"): preparing, then playing; null when none. */
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val replyPlayback: ReplyPlayback? = null,
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val readingAnchor: ReadingAnchor? = null,
    /** The selected conversation's accepted messages not yet echoed by the Mac ([Session.sent]). */
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val sent: List<SentMessage> = emptyList(),
    /** The selected conversation's rows that echo this phone's messages ([Session.echoes]). */
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val echoes: List<Echo> = emptyList(),
    /**
     * When the wait for the press on the Mac next needs the app's timer (`mac-wait`): the next ask,
     * or the bound. Null unless waiting AND on screen, so a hidden app holds no timer for it.
     */
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val macWaitDueInMs: Long? = null,
    /** The files Rich attached that were tapped on this phone, by id ([FileFetch]). */
    @OptIn(ExperimentalSerializationApi::class) @EncodeDefault(EncodeDefault.Mode.NEVER)
    val fileFetches: Map<String, FileFetch> = emptyMap(),
) {
    /**
     * What the gold circle in the composer shows: the microphone becomes the send arrow
     * "the moment there is a draft" (round-12 `comp-typing`). A draft of only whitespace
     * keeps the microphone, because `send` refuses it (`Message is empty`, `app.js`).
     */
    val composerAction: ComposerAction
        get() = if (draft.isBlank() && pendingAttachments.isEmpty()) ComposerAction.RECORD else ComposerAction.SEND

    /** The selected conversation as it is drawn ([Transcript]). Derived, never stored. */
    val transcript: List<Line> get() = Transcript.of(this)

    companion object {
        fun of(session: Session, outbox: List<OutboxItem>, dueInMs: Long?, lastSend: SendReport?, connection: ConnectionState = ConnectionState()) = AppState(
            threads = session.threads,
            selectedThreadId = session.selectedThreadId,
            draft = session.draft,
            online = session.online,
            paired = session.paired,
            theme = session.theme,
            pairing = session.pairing,
            messages = session.selectedThreadId?.let { session.cache[it] }.orEmpty(),
            capabilities = session.capabilities,
            connection = connection,
            outbox = outbox,
            dueInMs = dueInMs,
            lastSend = lastSend,
            readingAnchor = session.readingAnchor,
            sent = session.sent.filter { it.threadId == session.selectedThreadId },
            echoes = session.echoes.filter { it.threadId == session.selectedThreadId },
        )
    }
}

enum class ComposerAction { RECORD, SEND }

/** Disposable history, persisted separately from drafts and send transactions. */
@Serializable
data class SavedHistory(
    val identity: String = "",
    val rows: Map<String, List<dev.richos.android.core.protocol.Row>> = emptyMap(),
    val older: Map<String, Boolean> = emptyMap(),
    val cursor: Long? = null,
)

@Serializable
data class ReadingAnchor(val messageId: String, val offset: Int = 0)
