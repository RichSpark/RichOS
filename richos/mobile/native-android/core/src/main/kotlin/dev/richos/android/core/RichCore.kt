package dev.richos.android.core

import dev.richos.android.core.protocol.Fingerprint
import dev.richos.android.core.protocol.MacApi
import dev.richos.android.core.protocol.MacNeedsPairV2
import dev.richos.android.core.protocol.MacWait
import dev.richos.android.core.protocol.NativePush
import dev.richos.android.core.protocol.PAIR_WAIT
import dev.richos.android.core.protocol.Delta
import dev.richos.android.core.protocol.Hello
import dev.richos.android.core.protocol.PairLink
import dev.richos.android.core.protocol.Row
import dev.richos.android.core.protocol.SseFrame
import dev.richos.android.core.protocol.SseItem
import dev.richos.android.core.protocol.SseParser
import kotlinx.serialization.json.Json
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.net.URI

/**
 * The application core: one instance per process, over injected [Ports]. The port of the
 * preserved phone core's `createApp` (`richos/mobile/core/app.js`): actions are serialized,
 * every accepted action is written through the [SessionStore] before it resolves, and
 * [state] retains the latest draft in memory if saving it fails; failed saves still throw.
 *
 * Network work never holds the lock: an action commits its "in progress" state, releases the
 * lock for the request, then re-takes it to commit the outcome — unless a later action has
 * superseded it, in which case the outcome is dropped. So typing is never stuck behind a slow Mac.
 *
 * Built so far: `select-thread`, `compose`, `network`, `theme`; pairing (`pair`,
 * `confirm-words`, `forget`, phone protocol contract §2); and text sending through the durable
 * [Outbox] (`send`, `sync`, `retry`, `discard`, the `queue.js` rules). `send-voice` is refused
 * with a sentence until the voice recording lifecycle lands.
 */
/**
 * The connection owner's view of a send ([ConnectionOwner]'s dead-stream check). [started] is
 * called as a text's request goes to the Mac and returns what to call if the Mac accepts it, or
 * null when there is nothing to watch (no stream open).
 */
fun interface SendWatch {
    fun started(): (() -> Unit)?
}

class RichCore private constructor(
    private val ports: Ports,
    private var session: Session,
    private val outbox: Outbox,
) {
    private val mutex = Mutex()
    private var lastSend: SendReport? = null
    private var connection = ConnectionState()
    private var unsupported = false
    /**
     * The files Rich attached that were tapped on this phone and where each stands (round 36
     * screen 3); not saved: a kept copy is found again on disk. Declared before [flow], whose first
     * snapshot reads it: Kotlin initializes properties in the order written.
     */
    private val fetches = LinkedHashMap<String, FileFetch>()

    /** The tap whose file is coming over, so × can stop it. */
    private val fetchJobs = HashMap<String, Job>()
    private val flow = MutableStateFlow(snapshot())
    private val api = MacApi(ports.http, ports.keys)
    private var freshChallenge: String? = null
    private val transport: Transport = ports.transport
        ?: MacTransport(api, { session.pairing }, { freshChallenge = it }, ports.files)
    private var generation = 0
    private var playingRecordingId: String? = null
    @Volatile private var visible = true

    /** The last committed state; the app collects this. */
    val states: StateFlow<AppState> = flow.asStateFlow()

    @Volatile private var tryNow: (() -> Unit)? = null

    /**
     * "Try now" pressed while the stream to the Mac is down (I06, parity with the iPhone's `retryNow`
     * emitting `connect`): the outbox cannot move without an open link, so the ask goes to the ONE
     * connection owner, which tries at once, skipping what is left of its wait without resetting
     * the back-off ([ConnectionOwner.wake]). Called in order with the action, so the owner never
     * sees it late. Nothing listens in a development world, and nothing is kept for later.
     */
    fun onTryNowWhileAway(listener: (() -> Unit)?) {
        tryNow = listener
    }

    @Volatile private var sendWatch: SendWatch? = null

    /** The connection owner's [SendWatch]: told when a text goes to the Mac and when it is accepted. */
    fun watchSends(watch: SendWatch?) {
        sendWatch = watch
    }

    val state: AppState get() = flow.value

    suspend fun dispatch(action: Action): AppState {
        mutex.withLock { finishEnqueues() }
        return dispatchReady(action)
    }

    private suspend fun dispatchReady(action: Action): AppState = when (action) {
        Action.PlayKept -> mutex.withLock {
            val kept = session.keptRecordings.lastOrNull()
            if (!visible || voiceSession != null || kept == null) return@withLock emit()
            endReply()
            if (playingRecordingId == kept.id) {
                ports.recorder.stopPlayback()
                playingRecordingId = null
            } else {
                ports.recorder.stopPlayback()
                playingRecordingId = null
                val started = try { ports.recorder.play(kept.id) } catch (failure: Throwable) { emit(); throw failure }
                if (started && visible) playingRecordingId = kept.id else ports.recorder.stopPlayback()
            }
            emit()
        }
        is Action.PlaybackEnded -> mutex.withLock {
            if (playingRecordingId == action.id) playingRecordingId = null
            if (reply?.id == action.id) endReply()
            emit()
        }
        is Action.HearReply -> hearReply(action.id)
        is Action.OpenRichFile -> openRichFile(action.id)
        is Action.StopRichFile -> mutex.withLock {
            fetchJobs.remove(action.id)?.cancel()
            if (fetches[action.id]?.state == FileFetchState.COMING || fetches[action.id]?.state == FileFetchState.WAITING) fetches.remove(action.id)
            emit()
        }
        is Action.RichFileViewed -> mutex.withLock {
            fetches[action.id]?.let { fetches[action.id] = it.copy(open = false) }
            emit()
        }
        is Action.StopReply -> mutex.withLock {
            if (reply?.id == action.id) {
                endReply()
                ports.recorder.stopPlayback()
            }
            emit()
        }
        is Action.Pair -> pair(action.link)
        is Action.ConfirmWords -> confirmWords(action.match)
        Action.MacWait -> macWait()
        Action.Forget -> forget()
        is Action.AnswerQuestion -> answerQuestion(action)
        Action.Send -> send()
        Action.Sync -> if (session.paired) flush() else mutex.withLock { emit() }
        Action.Tick -> voice(action)
        is Action.VoicePress, is Action.VoiceStartLocked, is Action.MicrophonePermission, is Action.VoiceMove,
        is Action.VoiceRelease, is Action.VoiceLockedSend, is Action.VoiceLockedCancel, is Action.VoiceTouchCanceled,
        is Action.VoiceInterrupted, is Action.VoiceLevel, Action.VoiceSettled, is Action.SendKept, is Action.DiscardKept,
        Action.AskMicrophone, Action.DismissMicrophoneCard -> voice(action)
        Action.TurnOnNotifications, is Action.NotificationsResult, Action.TurnOffNotifications, Action.DismissNotificationOffer,
        is Action.SetPreviews, is Action.OpenSheet, Action.CloseSheet, Action.ForgetPairing, Action.ConfirmForget, Action.DiscardAndForget,
        Action.OpenSystemSettings, is Action.OpenedFromNotification, is Action.ProvisionalReply, Action.ClearFocus, is Action.UpdatePolicy,
        Action.DismissUpdate, Action.OpenAppStore, Action.OpenSupport, Action.CheckForUpdates, Action.OpenPrivacyPolicy -> settings(action)
        is Action.PushToken -> pushToken(action)
        Action.LoadOlder -> loadOlder()
        is Action.SendAttachments -> sendAttachments(action)
        is Action.Share -> {
            mutex.withLock {
                if (!session.paired) throw CoreError("Pair this phone with your Mac first")
                val text = action.text.trim()
                if (action.files.isEmpty()) {
                    if (text.isEmpty()) throw CoreError("There is nothing to send")
                    val threadId = session.selectedThreadId ?: throw CoreError("Choose a conversation before sending")
                    val sentAt = isoMillis(ports.clock.now())
                    enqueue(OutboxItem(clientId = action.clientId, threadId = threadId, kind = "text", text = text, queuedAt = sentAt,
                        wire = Wire.text(action.clientId, threadId, text, sentAt)))
                } else {
                    checkAttachments(action.files)
                    enqueue(enqueueAttachments(action.files, text).copy(clientId = action.clientId).let { item ->
                        item.copy(wire = Wire.attachments(action.clientId, item.threadId, text, action.files, item.queuedAt))
                    })
                }
                emit()
            }
            flush()
        }
        is Action.Attach -> mutex.withLock { take(action.files) }
        is Action.PickAttachments -> pick(action.source)
        is Action.AttachRefused -> mutex.withLock {
            attachNotice = AttachNotice.Refused(action.name, action.bytes, action.tooLarge)
            emit()
        }
        is Action.AttachPermissionDenied -> mutex.withLock {
            if (action.source == AttachSource.CAMERA) attachNotice = AttachNotice.CameraDenied
            emit()
        }
        Action.DismissAttachNotice -> mutex.withLock {
            attachNotice = null
            emit()
        }
        is Action.RemoveAttachment -> mutex.withLock {
            if (session.pendingAttachments.none { it.id == action.id }) return@withLock emit()
            ports.files.delete(action.id)
            commit(session.copy(pendingAttachments = session.pendingAttachments.filter { it.id != action.id }))
        }
        is Action.Link -> link(action.status)
        is Action.Health -> mutex.withLock {
            // The OS's report of a VPN on the default network (D05): kept as evidence, whenever it comes.
            action.vpn?.let { vpn = it }
            val diagnosis = action.phoneOnline != null || action.service != null || action.vpn == null
            // Only while away: a probe that lands after the socket reopened is stale.
            if (!session.online && diagnosis) {
                val reason = Connections.classify(false, revoked(), unsupported, action.phoneOnline, action.service)
                connection = connection.copy(
                    // A healthy relay does not prove the Mac is asleep or broken.
                    reason = if (reason == ConnectionReason.MAC_UNREACHABLE) ConnectionReason.RECONNECTING else reason,
                    troubleSince = connection.troubleSince ?: ports.clock.now(),
                )
            }
            emit()
        }
        Action.Retry -> {
            mutex.withLock {
                if (!session.paired) throw CoreError("Pairing has been revoked")
                outbox.retryEverythingNow()
                if (!session.online) tryNow?.invoke()
                emit()
            }
            flush()
        }
        is Action.Discard -> mutex.withLock {
            val files = outbox.all().firstOrNull { it.clientId == action.clientId }?.attachments.orEmpty()
            outbox.discard(action.clientId)
            files.forEach { ports.files.delete(it.id) }
            emit()
        }
        is Action.Network -> {
            mutex.withLock { commit(session.copy(online = action.online)) }
            if (action.online && session.paired) flush() else flow.value
        }
        // `app.js` `send-voice`: a recording saved elsewhere, queued as one voice message.
        is Action.SendVoice -> {
            mutex.withLock {
                if (!session.paired || session.selectedThreadId == null) throw CoreError("Pair this device before sending")
                enqueue(voiceItem(action.clientId ?: ports.ids.next(), action.threadId ?: session.selectedThreadId!!, action.recording.id, (action.recording.seconds * 1000).toLong(), emptyList()))
                emit()
            }
            flush()
        }
        is Action.Receive -> {
            mutex.withLock { receive(action.wire) }
            if (outbox.all().any { it.kind == "question_seen" }) flush() else flow.value
        }
        is Action.SelectThread -> mutex.withLock {
            if (session.threads.none { it.id == action.threadId }) throw CoreError("Unknown conversation")
            commit(session.copy(selectedThreadId = action.threadId, readingAnchor = null))
        }
        is Action.RememberReading -> mutex.withLock { commit(session.copy(readingAnchor = action.anchor)) }
        is Action.Compose -> mutex.withLock {
            val next = session.copy(draft = action.text)
            try {
                commit(next)
            } catch (failure: java.io.IOException) {
                // A failed save must not echo the older disk draft into the
                // editor or let a later Send use those older words. Keep the
                // edit in memory, report the failure and retry no work here.
                session = next
                emit()
                throw failure
            }
        }
        is Action.SetTheme -> mutex.withLock { commit(session.copy(theme = action.theme)) }
    }

    // --- the conversation, from the Mac's event stream (contract §5.4) -----------------------------

    private val sse = SseParser()
    private val lenient = Json { ignoreUnknownKeys = true }

    /** True once the Mac said this socket fell behind; the connection owner reconnects without `since`. */
    var resnapshotRequested: Boolean = false
        private set

    /** The live stream's bytes as they arrive, split anywhere (the connection owner's input). */
    suspend fun receive(bytes: ByteArray): AppState {
        mutex.withLock { apply(sse.feed(bytes)) }
        return if (outbox.all().any { it.kind == "question_seen" }) flush() else flow.value
    }

    /** When this process learned the challenge it holds, on the phone's clock; null for one read from disk. */
    @Volatile private var challengeLearnedAt: Long? = null

    /**
     * How long ago this process learned the challenge it holds, or null when it does not know (the
     * challenge came from disk, or the clock went backwards). The Mac honors a challenge for 10
     * minutes from issuing it (`device.rs` `CHALLENGE_LIFETIME_MS`); the connection owner presents a
     * young one as it is rather than asking for a fresh one first ([ConnectionOwner.CHALLENGE_REUSE_MS]).
     */
    fun challengeAgeMs(): Long? {
        if (session.pairing.challenge == null) return null
        val age = ports.clock.now() - (challengeLearnedAt ?: return null)
        return age.takeIf { it >= 0 }
    }

    /** A challenge learned outside a request the core made (the owner's refresh or probe). */
    suspend fun adoptChallenge(challenge: String): AppState = mutex.withLock {
        if (challenge == session.pairing.challenge) emit() else commit(session.copy(pairing = session.pairing.copy(challenge = challenge)))
    }

    /** The Mac answered 403 `{"revoked":true}` to a probe: this phone was removed from the Mac. */
    suspend fun markRevoked(): AppState = mutex.withLock {
        commit(session.copy(paired = false, pairing = session.pairing.copy(problem = "revoked")))
    }

    /**
     * The app left the screen and the connection owner closed the stream: no stream in the
     * background (the iPhone's `backgrounded`; build plan §3.2: push is for awareness, the
     * foreground reconciles). Not trouble: no notice falls due, and with the link closed nothing in
     * the outbox is owed a timed try, so nothing wakes the app until it returns or a person sends.
     */
    fun foregrounded() {
        // Back from hidden: the wait for the press on the Mac asks again rather than sitting out an
        // ask scheduled up to 15 s ahead (Sage's pair-v2 review §1; `pair_wait` "coming back asks
        // again"). Only on a real return: a network change while on screen is not one.
        if (!visible) askOnReturn = true
        visible = true
        outbox.foregrounded()
    }

    suspend fun backgrounded(): AppState {
        visible = false
        outbox.backgrounded()
        historyJob?.cancel()
        // The wait for the press on the Mac asks only while on screen: an ask in flight is
        // abandoned (it was already counted), and the state published below holds no due time,
        // so the app's timer holds nothing for it either (CEO ruling §81).
        macWaitJob?.cancel()
        ports.recorder.stopPlayback()
        return mutex.withLock {
        playingRecordingId = null
        endReply()
        // Lifecycle shutdown must take effect even when disk is full. An online flag is never a
        // reason to keep an outbox timer or network drain alive behind a hidden interface.
        session = session.copy(online = false)
        val keep = connection.reason == ConnectionReason.PHONE_OFFLINE || connection.reason == ConnectionReason.SERVICE_UNAVAILABLE
        val reason = when {
            keep -> connection.reason
            connection.hasConnected -> ConnectionReason.RECONNECTING
            else -> ConnectionReason.CONNECTING
        }
        connection = connection.copy(reason = reason, troubleSince = null)
        val action = Action.VoiceInterrupted(ports.clock.now())
        val recoverable = VoiceMachine.reduce(
            voiceWorld(),
            action, ports.clock.now(),
        ).first.kept
        try {
            voiceLocked(action)
            ports.session.write(session)
        } catch (failure: java.io.IOException) {
            // Capture has already been asked to stop. Retain its recovery card in memory as well
            // as the audio file; the app reports the failed save instead of losing the work silently.
            session = session.copy(keptRecordings = recoverable)
            throw failure
        } finally { emit() }
        flow.value
        }
    }

    private suspend fun receive(wire: String): AppState = apply(sse.feed(wire))

    private suspend fun apply(items: List<SseItem>): AppState {
        var next = session
        for (item in items) {
            when (item) {
                is SseItem.Frame -> {
                    if (item.frame.event == "delta") ports.performance.mark("text-received")
                    next = apply(next, item.frame)
                    // A `hello` is never a place to resume from: its id is the hub's cursor, which
                    // the Mac may have seeded from the conversation's row count above every frame it
                    // holds, and a resume from there (`since` = id - 1) is answered with an EMPTY
                    // opening that the Mac's listener sends nothing for until its keep-alive, 15 s
                    // later (`app/src-tauri/src/phone/stream.rs` `replay_after_marked`, `listen.rs`
                    // `KEEPALIVE_MS`; the iPhone's 15.3 s return, Isaac 980c49473). So after a
                    // `hello` the next stream asks for a `hello` (no cursor, see
                    // [ConnectionOwner.eventsPath]), which is never empty; a live frame sets it again.
                    if (item.frame.event == "hello") next = next.copy(streamCursor = null)
                    else item.frame.id?.toLongOrNull()?.let { next = next.copy(streamCursor = it) }
                }
                SseItem.KeepAlive -> Unit
                is SseItem.Resnapshot -> resnapshotRequested = true
            }
        }
        val known = session.cache.values.flatten().mapNotNull { it.question?.id }.toSet()
        for (q in next.cache.values.flatten().mapNotNull { it.question }.distinctBy { it.id }.filter { it.id !in known }) {
            val id = "seen-question:" + q.id
            val wire = kotlinx.serialization.json.buildJsonObject {
                put("kind", kotlinx.serialization.json.JsonPrimitive("question_seen")); put("client_id", kotlinx.serialization.json.JsonPrimitive(id))
                put("question_id", kotlinx.serialization.json.JsonPrimitive(q.id)); put("thread_id", kotlinx.serialization.json.JsonPrimitive(q.threadId))
            }.toString()
            enqueue(OutboxItem(clientId = id, threadId = q.threadId, kind = "question_seen", text = "", queuedAt = isoMillis(ports.clock.now()), wire = wire))
        }
        return if (next != session) commit(Echoes.reconcile(next)) else emit()
    }

    private fun <T> decode(serializer: kotlinx.serialization.KSerializer<T>, data: String): T? =
        runCatching { lenient.decodeFromString(serializer, data) }.getOrNull()

    private fun apply(s: Session, frame: SseFrame): Session = when (frame.event) {
        "hello" -> decode(Hello.serializer(), frame.data)?.let { h ->
            resnapshotRequested = false
            // The preserved core's rule: a protocol_version other than 1 means "use compatible
            // versions"; none at all is a Mac that predates the field and speaks version 1.
            unsupported = h.protocolVersion != null && h.protocolVersion != 1L
            resnapshotRequested = unsupported
            val thread = h.threadId ?: s.selectedThreadId
            val held = ProvisionalReply.retire(s.cache[thread].orEmpty(), h.messages)
            // A fresh snapshot covers only the newest page. Keep the contiguous loaded history
            // while the person is reading it, replacing overlapping rows with authoritative ones.
            val keepReading = thread == s.selectedThreadId && held.any { it.id == s.readingAnchor?.messageId }
            val rows = if (keepReading) (held.filter { old -> h.messages.none { it.id == old.id } } + h.messages).sortedBy { it.cursor }
                else bounded(h.messages)
            s.copy(
                threads = h.threads.ifEmpty { s.threads },
                selectedThreadId = s.selectedThreadId ?: thread,
                capabilities = h.capabilities,
                macBuild = h.build ?: s.macBuild,
                attachmentLimits = if ("attachments" in h.capabilities) h.attachmentLimits ?: s.attachmentLimits else null,
                pairing = h.challenge?.let { s.pairing.copy(challenge = it) } ?: s.pairing,
                cache = if (thread == null) s.cache else s.cache + (thread to rows),
                // The Mac holds older rows exactly when the oldest one it sent is not cursor 1.
                olderAvailable = if (thread == null) s.olderAvailable else s.olderAvailable + (thread to ((rows.minOfOrNull { it.cursor } ?: 1L) > 1L)),
            )
        } ?: s
        "message" -> decode(Row.serializer(), frame.data)?.let { row ->
            val held = ProvisionalReply.retire(s.cache[row.threadId].orEmpty(), listOf(row))
            // A finished reply is final. A reconnect replays the last reply whole (its opening row,
            // deltas and completion share one frame id on the Mac, and `since` is inclusive), so
            // an arriving copy of a row held finished is that replay, never news (D02).
            if (!row.complete && held.any { it.id == row.id && it.complete }) return@let s
            // A message typed at the Mac arrives first as the Mac's stand-in (`intake_<n>`,
            // `phone/stream.rs` `announce_his_words`) and then as the turn's own row: the
            // stand-in retires when that row arrives, matched by its words (the iPhone's
            // `ThreadModel.merge`). Only on arrival, so an older row never retires a newer one.
            val retired = if (row.role == "ceo" && !Echoes.isStandIn(row)) held.filter { Echoes.isStandIn(it) && it.text == row.text } else emptyList()
            val merged = held.filter { it.id != row.id && it !in retired } + row
            val keepReading = row.threadId == s.selectedThreadId && held.any { it.id == s.readingAnchor?.messageId }
            s.copy(cache = s.cache + (row.threadId to bounded(merged, if (keepReading) merged.size else held.size)))
        } ?: s
        "delta" -> decode(Delta.serializer(), frame.data)?.let { d ->
            // A delta names its conversation on newer Macs (Echo e9b0a89e): it lands only there.
            val thread = (if (d.threadId != null) d.threadId.takeIf { t -> s.cache[t].orEmpty().any { it.id == d.messageId } }
                else s.cache.entries.firstOrNull { (_, rows) -> rows.any { it.id == d.messageId } }?.key) ?: return@let s
            s.copy(cache = s.cache + (thread to s.cache.getValue(thread).map { if (it.id == d.messageId && !it.complete) it.copy(text = it.text + d.text) else it }))
        } ?: s
        else -> s
    }

    /**
     * The newest [Session.CACHE_ROWS] rows — or, once older history was loaded by scrolling up,
     * never fewer than were already held, so a live message never evicts what was just loaded.
     */
    private fun bounded(rows: List<Row>, held: Int = 0): List<Row> =
        rows.sortedBy { it.cursor }.distinctBy { it.id }.takeLast(maxOf(Session.CACHE_ROWS, held))

    // Live arrivals can evict the beginning without another hello. Derive that boundary from
    // the retained cursors too, including caches written by builds that kept a stale false flag.
    private fun hasOlder(thread: String?): Boolean = thread != null &&
        (session.olderAvailable[thread] == true || (session.cache[thread]?.minOfOrNull { it.cursor } ?: 1L) > 1L)

    // --- sending (queue.js rules, via [Outbox]) --------------------------------------------------

    private suspend fun answerQuestion(action: Action.AnswerQuestion): AppState {
        mutex.withLock {
            if (!session.paired || "questions" !in session.capabilities) throw CoreError("This Mac cannot accept question answers yet")
            val q = session.cache[session.selectedThreadId].orEmpty().firstOrNull { it.question?.id == action.id }?.question ?: throw CoreError("This question is not in this conversation")
            val previous = outbox.all().firstOrNull { it.questionId == q.id && it.state != OutboxState.BLOCKED }
            if (previous != null && (session.online || previous.state != OutboxState.WAITING || previous.attempts != 0 || action.revision != q.revision)) return@withLock
            if ((action.options.isEmpty() && action.text.isBlank()) || (!q.multiple && action.options.size > 1) || action.options.distinct().size != action.options.size || action.options.any { id -> q.options.none { it.id == id } } || (!q.freeAnswer && action.text.isNotEmpty())) throw CoreError("Choose this question's options or write an answer")
            val clientId = previous?.clientId ?: ports.ids.next()
            val item = OutboxItem(clientId = clientId, threadId = q.threadId, kind = "answer", text = (q.options.filter { it.id in action.options }.map { it.label } + action.text).filter { it.isNotEmpty() }.joinToString("; "), queuedAt = isoMillis(ports.clock.now()), questionId = q.id, wire = Wire.answer(clientId, q, action.options, action.text, if (q.state == "answered") action.revision else null))
            if (previous == null) enqueueConsuming(listOf(item), session) else { outbox.replaceUnsent(item); emit() }
        }
        return flush()
    }

    private suspend fun send(): AppState {
        // Photos or files waiting in the composer go as round 12.1 orders them ("Order of sending"):
        // the photos as one album message, then each file as its own message; the draft's words
        // ride on the album, or on the last file when there are no photos.
        if (session.pendingAttachments.isNotEmpty()) {
            mutex.withLock {
                val pending = session.pendingAttachments
                val photos = pending.filter { it.isPhoto }
                val groups = (if (photos.isEmpty()) emptyList() else listOf(photos)) + pending.filterNot { it.isPhoto }.map { listOf(it) }
                // Every message is checked before any is queued: all of them go, or none.
                groups.forEach { checkAttachments(it) }
                val words = session.draft.trim()
                val items = groups.mapIndexed { i, group ->
                    val carries = if (photos.isEmpty()) i == groups.lastIndex else i == 0
                    enqueueAttachments(group, if (carries) words else "")
                }
                enqueueConsuming(items, session.copy(draft = "", pendingAttachments = emptyList(), readingAnchor = null))
                attachNotice = null
            }
            return flush()
        }
        mutex.withLock {
            if (!session.paired) throw CoreError("Pair this device before sending")
            val text = session.draft.trim()
            if (text.isEmpty()) throw CoreError("Message is empty")
            val threadId = session.selectedThreadId ?: throw CoreError("Choose a conversation before sending")
            val clientId = ports.ids.next()
            val sentAt = isoMillis(ports.clock.now())
            enqueueConsuming(listOf(OutboxItem(
                clientId = clientId, threadId = threadId, kind = "text", text = text,
                queuedAt = sentAt, wire = Wire.text(clientId, threadId, text, sentAt),
            )), session.copy(draft = "", readingAnchor = null))
        }
        return flush()
    }

    /** The journal and consumed input commit together; replay uses the same stable client IDs. */
    private suspend fun enqueueConsuming(items: List<OutboxItem>, consumed: Session) {
        ports.performance.mark("send-requested")
        commit(consumed.copy(pendingEnqueues = items.map(::withBoundary)))
        ports.performance.mark("durable-queued")
        finishEnqueues()
    }

    /** Every queued message records its echo boundary as it enters the outbox. */
    private suspend fun enqueue(item: OutboxItem): OutboxItem = outbox.enqueue(withBoundary(item))

    private suspend fun finishEnqueues() {
        if (session.pendingEnqueues.isEmpty()) return
        for (item in session.pendingEnqueues) enqueue(item)
        commit(session.copy(pendingEnqueues = emptyList()))
    }

    /**
     * The newest of the Mac's rows on screen now, recorded on a message as it is queued: no row
     * at or before it can be that message's echo ([Echoes]). Kept if already recorded.
     */
    private fun withBoundary(item: OutboxItem): OutboxItem {
        if (item.echoAfterCursor != null || item.echoAfterMessageId != null) return item
        val (cursor, id) = Echoes.boundary(session.cache[item.threadId].orEmpty())
        return item.copy(echoAfterCursor = cursor, echoAfterMessageId = id)
    }

    /**
     * The Mac accepted [item]: it stays on screen, as sent, until the Mac's own row for it arrives
     * (D01). Recorded BEFORE the outbox drops the item, so no instant, and no relaunch, has it in
     * neither place. A failed save keeps it in memory: the Mac has the message whatever the disk
     * says, and the outbox's own removal is what makes delivery exactly-once.
     */
    private suspend fun accepted(item: OutboxItem, receipt: Receipt) = mutex.withLock {
        if (item.kind == "answer") {
            if (receipt.outcome == "conversation_deleted") throw TransportFailure("This conversation was deleted. Your answer is saved on this phone.", retryable = false, aboutThisMessage = true)
            val q = receipt.question ?: throw TransportFailure("The Mac has not confirmed your answer", retryable = true)
            val rows = session.cache[item.threadId].orEmpty().map { if (it.question?.id == q.id) it.copy(question = q) else it }
            commit(session.copy(cache = session.cache + (item.threadId to rows)))
            return@withLock
        }
        if (item.kind == "question_seen") return@withLock
        if (session.sent.any { it.clientId == item.clientId } || session.echoes.any { it.clientId == item.clientId }) return@withLock
        val hash = receipt.textSha256?.lowercase()?.takeIf { h -> h.length == 64 && h.all { it in '0'..'9' || it in 'a'..'f' } }
        // A voice message is matched by the transcript's hash only. Without one (an older Mac)
        // nothing could ever retire it, so it is not held; its echo still arrives as the Mac's row.
        if (item.kind == "voice" && hash == null) return@withLock
        val next = Echoes.reconcile(session.copy(sent = session.sent + SentMessage(
            clientId = item.clientId, threadId = item.threadId, kind = item.kind, text = item.text, queuedAt = item.queuedAt,
            echoAfterCursor = item.echoAfterCursor, echoAfterMessageId = item.echoAfterMessageId,
            transcriptSha256 = hash, seconds = item.seconds, attachments = item.attachments,
        )))
        try {
            commit(next)
        } catch (_: java.io.IOException) {
            session = next
            emit()
        }
    }

    /** One pass over the outbox, outside the lock, while online (`app.js` `drain`). */
    private suspend fun flush(): AppState {
        if (!session.online) return flow.value
        val before = outbox.all()
        val report = outbox.flush(lease = { reserveCompletion() }) { item ->
            if (item.kind in listOf("answer", "question_seen") && !visible) throw TransportFailure("background", retryable = true)
            // A typed text is the one send whose echo the Mac puts on the stream at once (its spine
            // permitting): the owner uses its acceptance as a check that the stream is alive.
            val accepted = if (item.kind == "text") sendWatch?.started() else null
            val receipt = when (item.kind) {
                "voice" -> transport.sendVoice(item)
                "attachments" -> transport.sendAttachments(item)
                else -> transport.sendText(item)
            }
            accepted?.invoke()
            accepted(item, receipt)
            receipt
        }
        // A photo or file the Mac accepted is the Mac's now: the phone's staged copy goes.
        val left = outbox.all().map { it.clientId }.toSet()
        before.filter { it.clientId !in left }.flatMap { it.attachments.orEmpty() }.forEach { ports.files.delete(it.id) }
        return mutex.withLock {
            lastSend = report
            // Every response's challenge replaces the one held (contract §3.3).
            val challenge = freshChallenge?.takeIf { it != session.pairing.challenge }
            freshChallenge = null
            val pairing = session.pairing.let { p -> if (challenge != null) p.copy(challenge = challenge) else p }
            if (report.reason == "revoked") {
                commit(session.copy(paired = false, pairing = pairing.copy(problem = "revoked")))
            } else if (pairing != session.pairing) {
                commit(session.copy(pairing = pairing))
            } else {
                emit()
            }
        }
    }

    /** Five seconds / three small text requests per used lease; six leases/hour, 24/day.
     * Reserve before network I/O. A crash spends the reservation; clock rollback cannot refill it.
     */
    private suspend fun reserveCompletion(): Outbox.CompletionLease = mutex.withLock {
        val now = ports.clock.now()
        val retained = session.completionReservations.filter { now - it < 86_400_000L }
        if (retained.size >= 24 || retained.count { now - it < 3_600_000L } >= 6)
            return@withLock Outbox.CompletionLease(false)
        val token = maxOf(now, (retained.maxOrNull() ?: (now - 1)) + 1)
        try { commit(session.copy(completionReservations = retained + token)) }
        catch (_: java.io.IOException) { return@withLock Outbox.CompletionLease(false) }
        Outbox.CompletionLease(true) { used ->
            if (!used) mutex.withLock {
                try { commit(session.copy(completionReservations = session.completionReservations - token)) }
                catch (_: java.io.IOException) { /* Keep the reservation. Never retry optional accounting on a timer. */ }
            }
        }
    }

    // --- older messages (contract §5.5) ----------------------------------------------------------

    private var loadingOlder = false
    @Volatile private var historyJob: Job? = null

    private suspend fun loadOlder(): AppState = coroutineScope {
        val operation = currentCoroutineContext()[Job]!!
        var mine = 0
        val request = mutex.withLock {
            val thread = session.selectedThreadId
            val p = session.pairing
            val oldest = thread?.let { session.cache[it] }?.minOfOrNull { it.cursor }
            if (!visible || !session.online || loadingOlder || thread == null || oldest == null || !hasOlder(thread) || !session.paired ||
                p.apiBase == null || p.deviceId == null || p.challenge == null
            ) {
                return@withLock null
            }
            loadingOlder = true
            historyJob = operation
            mine = generation
            emit()
            Triple(thread, oldest, p)
        } ?: return@coroutineScope flow.value
        val (thread, oldest, p) = request
        try {
            val result = withTimeoutOrNull(10_000) { historyPage(p, thread, oldest) }
            if (result != null) mutex.withLock {
                if (!visible || generation != mine || session.pairing.apiBase != p.apiBase) return@withLock
                val (answer, challenge) = result
                val held = session.cache[thread].orEmpty()
                val merged = bounded(answer.messages + held, held.size + answer.messages.size)
                commit(Echoes.reconcile(session.copy(
                    cache = session.cache + (thread to merged),
                    olderAvailable = session.olderAvailable + (thread to answer.more),
                    pairing = session.pairing.copy(challenge = challenge),
                )))
            }
        } finally {
            withContext(NonCancellable) {
                mutex.withLock {
                    if (historyJob === operation) {
                        historyJob = null
                        loadingOlder = false
                        emit()
                    }
                }
            }
        }
        flow.value
    }

    private suspend fun historyPage(pairing: Pairing, thread: String, oldest: Long): Pair<dev.richos.android.core.protocol.Backfill, String>? {
        for (attempt in 1..3) {
            currentCoroutineContext().ensureActive()
            if (!visible || !session.online) return null
            try { return api.backfill(pairing.apiBase!!, pairing.deviceId!!, pairing.challenge!!, thread, oldest) }
            catch (failure: TransportFailure) {
                if (!failure.retryable || attempt == 3) return null
                delay(Outbox.retryDelayMs(attempt))
            }
        }
        return null
    }

    // --- "Hear it": Rich's reply read aloud by the Mac (round 12.1 `richAudioHTML`) ---------------

    /** The reply being heard, preparing or playing; null when none. */
    private var reply: ReplyPlayback? = null

    /** Counts presses and stops, so audio that arrives for a press no longer wanted never plays. */
    private var replyAsk = 0

    private fun endReply() {
        reply = null
        replyAsk++
    }

    /**
     * "Hear it" (`app.js` `hearIt`): whatever plays stops, the reply shows "Preparing the audio…", the
     * Mac's audio comes from ONE signed `GET /api/audio/<id>?thread_id=<thread>` (never retried, so
     * nothing runs that the person did not ask for), and it plays with Stop and the waveform. A
     * refusal, no answer, bytes over the Mac's ceiling or a player that cannot start puts "Hear it"
     * back. A Stop, another press, a recording or leaving the screen meanwhile wins: nothing plays.
     */
    private suspend fun hearReply(id: String): AppState {
        val ask = mutex.withLock {
            val p = session.pairing
            val row = session.selectedThreadId?.let { session.cache[it] }?.lastOrNull { it.id == id }
            if (!visible || voiceSession != null || row == null || !session.paired || !ReplyAudioRule.hearable(row, session.capabilities) ||
                p.apiBase == null || p.deviceId == null || p.challenge == null || reply?.id == id
            ) {
                return@withLock null
            }
            ports.recorder.stopPlayback()
            playingRecordingId = null
            endReply()
            reply = ReplyPlayback(id, playing = false)
            emit()
            Triple(replyAsk, p, row.threadId)
        } ?: return flow.value
        val (mine, p, thread) = ask
        var challenge: String? = null
        val audio = try {
            withTimeoutOrNull(ReplyAudioRule.REQUEST_MS) {
                val signed = api.signed(p.apiBase!!, p.deviceId!!, p.challenge!!, "GET", ReplyAudioRule.path(id, thread), null)
                challenge = signed.challenge
                signed.response.body.takeIf { it.isNotEmpty() && it.size <= ReplyAudioRule.MAX_BYTES }
            }
        } catch (failure: TransportFailure) {
            challenge = failure.challenge
            null
        }
        return mutex.withLock {
            if (replyAsk == mine) {
                val started = audio != null && visible && voiceSession == null &&
                    try { ports.recorder.playReply(id, audio) } catch (failure: CoreError) { false }
                if (started && visible) {
                    reply = ReplyPlayback(id, playing = true)
                } else {
                    if (started) ports.recorder.stopPlayback()
                    endReply()
                }
            }
            // The newest challenge the Mac gave, kept like every signed answer's (after the outcome,
            // so a storage failure here can never leave "Preparing the audio…" on screen).
            val fresh = challenge?.takeIf { it != session.pairing.challenge && session.pairing.apiBase == p.apiBase }
            if (fresh != null) commit(session.copy(pairing = session.pairing.copy(challenge = fresh))) else emit()
        }
    }

    // --- a file Rich put in front of the user (round 36 screen 3) ---------------------------------
    // (`fetches` and `fetchJobs` are declared at the top of the class: `flow` reads `fetches`.)

    private fun fileRow(id: String): Row? = session.cache.values.asSequence().flatten().lastOrNull { it.file?.id == id }

    /**
     * A tap on one of Rich's files (round 36 "Phone: in the chat"): a copy already on this phone opens
     * at once; otherwise it comes over from the Mac on ONE signed `GET /api/messages?kind=file`, the
     * ring filling as the bytes arrive, and opens when it is here. With the Mac out of reach it waits
     * (the clock; "Opens when your Mac is back online") and the app asks again when the link opens. A
     * file the Mac says is gone, or too large, says the Mac's own sentence. × stops it, and nothing
     * half-written is kept.
     */
    private suspend fun openRichFile(id: String): AppState {
        val ask = mutex.withLock {
            val row = fileRow(id)
            val file = row?.file ?: return@withLock null
            if (fetches[id]?.state == FileFetchState.COMING) return@withLock null
            ports.richFiles.kept(id, file.name)?.let { path ->
                fetches[id] = FileFetch(id, FileFetchState.KEPT, file.bytes ?: 0, file.bytes, path = path, open = true)
                emit()
                return@withLock null
            }
            if (!file.exists) {
                fetches[id] = FileFetch(id, FileFetchState.FAILED, reason = RichFileRule.GONE)
                emit()
                return@withLock null
            }
            val p = session.pairing
            if (!session.paired || !session.online || p.apiBase == null || p.deviceId == null || p.challenge == null) {
                fetches[id] = FileFetch(id, FileFetchState.WAITING, total = file.bytes, open = true)
                emit()
                return@withLock null
            }
            fetches[id] = FileFetch(id, FileFetchState.COMING, 0, file.bytes, open = true)
            currentCoroutineContext()[Job]?.let { fetchJobs[id] = it }
            emit()
            Triple(p, row.threadId, file)
        } ?: return flow.value
        val (p, thread, file) = ask
        val path = RichFileRule.path(id, thread)
        var writer: RichFileWriter? = null
        val outcome: FileFetch = try {
            writer = ports.richFiles.begin(id, file.name)
                ?: return finishFetch(id, FileFetch(id, FileFetchState.FAILED, reason = RichFileRule.FAILED))
            var received = 0L
            var shown = 0L
            val step = maxOf(16_000L, (file.bytes ?: 0L) / 50)
            val status = fetchFile(p, path, { bytes ->
                writer.write(bytes)
                received += bytes.size
                if (received - shown >= step) {
                    shown = received
                    mutex.withLock {
                        if (fetches[id]?.state == FileFetchState.COMING) {
                            fetches[id] = fetches[id]!!.copy(received = received)
                            emit()
                        }
                    }
                }
            })
            when (status) {
                200 -> FileFetch(id, FileFetchState.KEPT, received, file.bytes ?: received, path = writer.finish(), open = true)
                410 -> FileFetch(id, FileFetchState.FAILED, reason = RichFileRule.GONE)
                413 -> FileFetch(id, FileFetchState.FAILED, reason = RichFileRule.TOO_LARGE)
                else -> FileFetch(id, FileFetchState.FAILED, reason = RichFileRule.FAILED)
            }
        } catch (stopped: kotlinx.coroutines.CancellationException) {
            withContext(NonCancellable) { writer?.abandon() }
            throw stopped
        } catch (failure: Exception) {
            withContext(NonCancellable) { writer?.abandon() }
            FileFetch(id, FileFetchState.FAILED, reason = RichFileRule.FAILED)
        }
        if (outcome.state != FileFetchState.KEPT) withContext(NonCancellable) { writer?.abandon() }
        return finishFetch(id, outcome)
    }

    private suspend fun finishFetch(id: String, outcome: FileFetch): AppState = mutex.withLock {
        fetchJobs.remove(id)
        // A × meanwhile wins: the tap it stopped leaves nothing behind.
        val open = fetches[id]?.open ?: return@withLock emit()
        fetches[id] = outcome.copy(open = open && outcome.state == FileFetchState.KEPT)
        emit()
    }

    /**
     * One signed GET for a file's bytes, in parts through the app's streaming GET when it has one
     * ([Ports.download]), else in one answer. Its status; the bytes go to [onBytes] only on a 200.
     * A 404 is the stale-challenge case the reference client re-signs once for ([MacApi.signed]).
     */
    private suspend fun fetchFile(p: Pairing, path: String, onBytes: suspend (ByteArray) -> Unit): Int {
        val apiBase = p.apiBase!!
        val auth = api.authorization(apiBase, p.deviceId!!, p.challenge!!, path)
        val request = dev.richos.android.core.protocol.HttpRequest("GET", apiBase + path, mapOf("Authorization" to auth), null)
        val first = ports.download?.let { stream ->
            var status = 0
            stream.open(request, { status = it }) { if (status == 200) onBytes(it) }
            status
        } ?: ports.http.send(request).let { response ->
            if (response.status == 200) onBytes(response.body)
            response.status
        }
        if (first != 404) return first
        return try {
            val signed = api.signed(apiBase, p.deviceId, p.challenge, "GET", path, null)
            onBytes(signed.response.body)
            200
        } catch (failure: TransportFailure) {
            404
        }
    }

    // --- photos and files (CEO §75; Echo 22e59ed8) ------------------------------------------------

    /** The Mac's limits, as sentences with the number that matters (round-12 `comp-too-long` style). */
    private fun checkAttachments(files: List<Attachment>): AttachmentLimits {
        if (!session.paired) throw CoreError("Pair this device before sending")
        val limits = session.attachmentLimits ?: throw CoreError("This Mac cannot take photos or files yet. Update RichOS on your Mac.")
        if (files.isEmpty()) throw CoreError("Choose a photo or a file to send")
        if (files.size > limits.maxFilesPerMessage) throw CoreError("Up to ${limits.maxFilesPerMessage} files in one message")
        files.firstOrNull { it.size > limits.maxFileBytes }?.let { throw CoreError("${it.name} is over the ${limits.maxFileBytes / (1024 * 1024)} MB limit for one file") }
        if (files.sumOf { it.size } > limits.maxMessageBytes) throw CoreError("Up to ${limits.maxMessageBytes / (1024 * 1024)} MB in one message")
        if (limits.mediaTypes.isNotEmpty()) files.firstOrNull { it.mediaType !in limits.mediaTypes }?.let { throw CoreError("${it.name} is a kind of file this Mac does not take") }
        return limits
    }

    private var attachNotice: AttachNotice? = null

    /**
     * The + menu's choice: a Mac that takes no photos or files says so; a full tray says "Up to N at
     * a time"; otherwise the platform presents its picker for the room left (the camera: one).
     */
    private suspend fun pick(source: AttachSource): AppState {
        val room = mutex.withLock {
            if (!session.paired) return@withLock null
            val limits = session.attachmentLimits
            val room = limits?.let { it.maxFilesPerMessage - session.pendingAttachments.size }
            attachNotice = when {
                limits == null -> AttachNotice.MacUnsupported
                room!! <= 0 -> AttachNotice.Limit(limits.maxFilesPerMessage)
                else -> null
            }
            emit()
            room?.takeIf { it > 0 }
        } ?: return flow.value
        ports.picker.present(source, if (source == AttachSource.CAMERA) 1 else room)
        return flow.value
    }

    /**
     * Into the tray, in the order chosen, each checked against what the Mac advertised. An item
     * refused, or past the count, never enters the tray: its card or line says why, and its staged
     * copy is deleted. The photos travel as one album, so their total stays inside one message.
     */
    private suspend fun take(files: List<Attachment>): AppState {
        val limits = session.attachmentLimits
        if (!session.paired || limits == null) {
            if (session.paired) attachNotice = AttachNotice.MacUnsupported
            files.forEach { ports.files.delete(it.id) }
            return emit()
        }
        var tray = session.pendingAttachments
        for (f in files) {
            if (tray.any { it.id == f.id }) continue
            val refusal = when {
                tray.size >= limits.maxFilesPerMessage -> AttachNotice.Limit(limits.maxFilesPerMessage)
                f.size > limits.maxFileBytes -> AttachNotice.Refused(f.name, f.size, tooLarge = true)
                limits.mediaTypes.isNotEmpty() && f.mediaType !in limits.mediaTypes -> AttachNotice.Refused(f.name, f.size, tooLarge = false)
                f.isPhoto && tray.filter { it.isPhoto }.sumOf { it.size } + f.size > limits.maxMessageBytes -> AttachNotice.Refused(f.name, f.size, tooLarge = true)
                else -> null
            }
            if (refusal != null) {
                attachNotice = refusal
                ports.files.delete(f.id)
            } else {
                tray = tray + f
            }
        }
        return if (tray != session.pendingAttachments) commit(session.copy(pendingAttachments = tray)) else emit()
    }

    private fun enqueueAttachments(files: List<Attachment>, text: String): OutboxItem {
        val threadId = session.selectedThreadId ?: throw CoreError("Choose a conversation before sending")
        val clientId = ports.ids.next()
        val sentAt = isoMillis(ports.clock.now())
        return OutboxItem(
            clientId = clientId, threadId = threadId, kind = "attachments", text = text, queuedAt = sentAt,
            attachments = files, wire = Wire.attachments(clientId, threadId, text, files, sentAt),
        )
    }

    private suspend fun sendAttachments(action: Action.SendAttachments): AppState {
        mutex.withLock {
            checkAttachments(action.files)
            enqueue(enqueueAttachments(action.files, action.text.trim()))
            emit()
        }
        return flush()
    }

    // --- native push registration (contract §7.2; Echo 65952d16) -----------------------------------

    private suspend fun pushToken(action: Action.PushToken): AppState {
        val attempt = mutex.withLock {
            val n = session.notifications
            if (!session.paired || n.status == NotificationStatus.OFF) return@withLock null
            if (FCM !in session.capabilities) {
                commit(session.copy(notifications = n.copy(status = NotificationStatus.UNSUPPORTED)))
                return@withLock null
            }
            Triple(session.pairing, n, generation)
        } ?: return flow.value
        val (pairing, intended, epoch) = attempt
        val outcome = pushLane.withLock {
            runCatching {
                api.registerPush(pairing.apiBase!!, pairing.deviceId!!, pairing.challenge!!, NativePush(token = action.token, topic = ports.applicationId, previewKey = action.previewKey, previews = intended.previews))
            }
        }
        return mutex.withLock {
            val n = session.notifications
            // A response belongs to the pairing and notification choice that initiated it.
            // In particular, it must not turn notifications back on after Off or Forget.
            if (generation != epoch || session.pairing.deviceId != pairing.deviceId || n != intended) return@withLock emit()
            val (answer, challenge) = outcome.getOrNull() ?: (null to null)
            val failure = outcome.exceptionOrNull() as? TransportFailure
            val status = when {
                answer?.registered == true -> NotificationStatus.ON
                answer != null -> NotificationStatus.SERVICE_UNAVAILABLE
                failure?.retryable == false -> NotificationStatus.UNSUPPORTED
                else -> NotificationStatus.SERVICE_UNAVAILABLE
            }
            commit(
                session.copy(
                    notifications = n.copy(status = status, offerDismissed = n.offerDismissed || status == NotificationStatus.ON),
                    pushHostId = answer?.hostId ?: session.pushHostId,
                    pairing = challenge?.let { session.pairing.copy(challenge = it) } ?: session.pairing,
                ),
            )
        }
    }

    // --- pairing (contract §2) -----------------------------------------------------------------

    private suspend fun pair(text: String): AppState {
        val link = PairLink.parse(text)
        val mine = mutex.withLock {
            if (!outbox.isEmpty()) throw CoreError(UNSENT_BEFORE_PAIRING)
            if (session.pairing.phase == PairingPhase.EXCHANGING) throw CoreError("A pairing code is already on its way to the Mac")
            val previous = session.pairing.apiBase.takeIf { session.pairing.phase != PairingPhase.UNPAIRED && it != link.origin }
            // The old Mac's removal ends here: a probe that ran while this phone stood removed stored
            // REVOKED as the link's reason, and kept, it held "removed from your Mac" over the new
            // pairing's six words (the Android test phone, 2026-10-22).
            if (connection.reason == ConnectionReason.REVOKED) connection = ConnectionState()
            commit(
                session.copy(
                    paired = false,
                    threads = emptyList(),
                    selectedThreadId = null,
                    cache = emptyMap(), olderAvailable = emptyMap(), streamCursor = null, readingAnchor = null,
                    sent = emptyList(), echoes = emptyList(),
                    pairing = Pairing(phase = PairingPhase.EXCHANGING, apiBase = link.origin, route = routeOf(link.origin)),
                ),
            )
            // Pairing with a different Mac replaces the old pairing, key included.
            previous?.let { ports.keys.delete(it) }
            ++generation
        }
        // THE ORIGIN THIS PHONE DIALED and this phone's own key: the v2 words are over both, so a
        // relay, or a device that redeemed the code first, shows a different line (Sage F2).
        val outcome = runCatching {
            val point = ports.keys.publicPoint(link.origin)
            val answer = api.pair(link, ports.deviceName, point)
            answer to Fingerprint.wordsV2(link.origin, answer.caFingerprint, Fingerprint.pointB64url(point))
        }
        // A MAC WITHOUT `pair-v2` IS REFUSED, NEVER FALLEN BACK TO (review §3.5). It has just
        // registered this key, so it is told to forget it with the one signed request this phone
        // can still make, and then the key goes here too (`app.js` `startPairing`). The phase is
        // still `exchanging`, so no other pairing can start meanwhile.
        val tooOld = outcome.exceptionOrNull() as? MacNeedsPairV2
        if (tooOld != null) {
            runCatching { api.confirm(link.origin, tooOld.answer.deviceId, tooOld.answer.challenge, false) }
            if (mutex.withLock { generation == mine }) ports.keys.delete(link.origin)
        }
        return mutex.withLock {
            if (generation != mine || session.pairing.phase != PairingPhase.EXCHANGING) return@withLock flow.value
            val (answer, words) = outcome.getOrNull() ?: (null to null)
            if (answer == null || words == null) {
                val problem = if (tooOld != null) PROBLEM_MAC_NEEDS_UPDATE else (outcome.exceptionOrNull() as? TransportFailure)?.reason ?: "fault"
                commit(session.copy(pairing = Pairing(apiBase = link.origin, route = routeOf(link.origin), problem = problem)))
            } else {
                commit(
                    session.copy(
                        threads = answer.threads,
                        selectedThreadId = answer.threadId ?: answer.threads.firstOrNull()?.id,
                        capabilities = answer.capabilities ?: session.capabilities,
                        macBuild = answer.build ?: session.macBuild,
                        attachmentLimits = answer.attachmentLimits?.takeIf { "attachments" in answer.capabilities.orEmpty() },
                        paired = false,
                        pairing = Pairing(
                            phase = PairingPhase.CONFIRMING,
                            apiBase = link.origin,
                            route = routeOf(link.origin),
                            deviceId = answer.deviceId,
                            caFingerprint = answer.caFingerprint,
                            words = words,
                            challenge = answer.challenge,
                            macWaitBoundMs = MacWait.boundMs(answer.confirmWithinSeconds),
                        ),
                    ),
                )
            }
        }
    }

    /**
     * The person's answer to the six words on the PHONE. "They match" tells the Mac, and then,
     * unless the Mac says its own press already happened, waits for the person to press
     * "They match" ON THE MAC (Sage F1): the Mac answers everything "waiting" until then. "They
     * do not match" works while confirming and while waiting: the Mac is told first, while the key
     * that signs the message still exists, then the key goes (contract §2.5).
     */
    private suspend fun confirmWords(match: Boolean): AppState {
        if (match) return pressTheyMatch()
        val (pending, _, _) = mutex.withLock {
            val p = session.pairing
            val answerable = p.phase == PairingPhase.CONFIRMING || p.phase == PairingPhase.AWAITING_MAC
            if (!answerable || p.apiBase == null || p.deviceId == null || p.challenge == null) {
                throw CoreError("There is no pairing waiting for its six words")
            }
            // "They do not match": the pairing is gone from the phone on the same press (contract
            // §2.5), a held ask with it, and the screen says so: the person may have caught a relay,
            // and the plain intro would make that security decision look like a reset (Urban's
            // review, state 4). The next scan clears it. The Mac is told, best effort, and then the
            // key is discarded.
            macWaitJob?.cancel()
            commit(session.copy(paired = false, threads = emptyList(), selectedThreadId = null,
                pairing = Pairing(apiBase = p.apiBase, route = p.route, problem = PROBLEM_WORDS_REJECTED)))
            Triple(p, ++generation, FCM in session.capabilities)
        }
        val apiBase = pending.apiBase!!
        runCatching { api.confirm(apiBase, pending.deviceId!!, pending.challenge!!, false) }
        // Unless this phone started pairing with the same Mac again meanwhile.
        if (mutex.withLock { session.pairing.phase == PairingPhase.UNPAIRED }) ports.keys.delete(apiBase)
        return flow.value
    }

    // --- waiting for the press on the Mac (pairing v2, Sage's review F1; `pair_wait`) -------------

    /** The ask in flight, if any (the press included): one at a time, and abandoned when the app leaves the screen. */
    @Volatile private var macWaitJob: Job? = null

    /** The app came back on screen (or the process restarted) during the wait: ask again without sitting out the schedule. */
    @Volatile private var askOnReturn = false

    /**
     * **"They match" ON THE PHONE IS THE FIRST ASK** (`pair_wait`: "The press itself is the first
     * ask"; `app.js` `pair-confirm`). The deadline counts from this press and is written down, with
     * the ask counted, BEFORE the request goes, so a restart in the middle of a held press resumes
     * the wait rather than showing the six words again. A Mac naming [PAIR_WAIT] holds the press
     * until its own press; the screen says which press is missing only once the Mac is taking its
     * time ([SHOW_WAITING_AFTER_MS]), so a Mac pressed first goes straight in with no flash.
     */
    private suspend fun pressTheyMatch(): AppState = coroutineScope {
        val operation = currentCoroutineContext()[Job]!!
        val ask = mutex.withLock {
            val p = session.pairing
            // A second tap while the press is out sends nothing (the button is still on screen).
            if (p.phase == PairingPhase.CONFIRMING && p.awaitingUntil != null) return@withLock null
            if (p.phase != PairingPhase.CONFIRMING || p.apiBase == null || p.deviceId == null || p.challenge == null) {
                throw CoreError("There is no pairing waiting for its six words")
            }
            val now = ports.clock.now()
            val until = now + (p.macWaitBoundMs ?: MacWait.WINDOW_MS)
            macWaitJob = operation
            askOnReturn = false
            val pressed = p.copy(problem = null, awaitingUntil = until, macAsks = 1, lastAskAt = now,
                nextAskAt = now + MacWait.nextDelayMs(0, 0, holds()))
            commit(session.copy(pairing = pressed))
            Ask(pressed, ++generation, now, final = false, wait = if (holds()) MacWait.preferWaitSeconds(now, until) else 0)
        } ?: return@coroutineScope flow.value
        val shown = launch {
            delay(SHOW_WAITING_AFTER_MS)
            mutex.withLock {
                val p = session.pairing
                if (generation == ask.generation && p.phase == PairingPhase.CONFIRMING && p.awaitingUntil != null) {
                    commit(session.copy(pairing = p.copy(phase = PairingPhase.AWAITING_MAC)))
                }
            }
        }
        try {
            answered(ask, askMac(ask), operation)
        } finally {
            shown.cancel()
            // However the press ended (answered, abandoned by leaving the screen), a pressed pairing
            // still on the six words is a wait, and the words stay up.
            withContext(NonCancellable) {
                mutex.withLock {
                    released(operation)
                    val p = session.pairing
                    if (generation == ask.generation && p.phase == PairingPhase.CONFIRMING && p.awaitingUntil != null) {
                        commit(session.copy(pairing = p.copy(phase = PairingPhase.AWAITING_MAC)))
                    }
                }
            }
        }
    }

    /** One ask: the pairing it was sent for, the generation, when it went, whether it is the last, and its `Prefer` seconds. */
    private class Ask(val pairing: Pairing, val generation: Int, val at: Long, val final: Boolean, val wait: Int)

    private fun holds() = PAIR_WAIT in session.capabilities

    /** The phone's own signed "They match", held by a Mac that offers it; null when it had no answer in time. */
    private suspend fun askMac(ask: Ask): Result<dev.richos.android.core.protocol.Confirmation>? {
        val p = ask.pairing
        val fcm = FCM in session.capabilities
        return withTimeoutOrNull(MAC_WAIT_REQUEST_MS) {
            runCatching { api.macAnswer(p.apiBase!!, p.deviceId!!, p.challenge!!, fcm, ask.wait) }
        }.also { currentCoroutineContext().ensureActive() }
    }

    /**
     * What an ask's answer means (`pair_wait.answers`): pressed is paired; a refusal is the Mac's
     * final answer ("They do not match" there, or its window closed), declined, never "removed from
     * your Mac"; at the LAST ask anything else means this phone did not hear back, expired; and
     * otherwise the wait goes on, the next ask at [MacWait.nextAskAt].
     */
    private suspend fun answered(ask: Ask, result: Result<dev.richos.android.core.protocol.Confirmation>?, operation: Job): AppState {
        var forget: String? = null
        val state = mutex.withLock {
            // The ask is over before its outcome is published, so that state carries the next due time.
            if (macWaitJob === operation) macWaitJob = null
            val now = session.pairing
            val pressed = now.awaitingUntil != null && (now.phase == PairingPhase.AWAITING_MAC || now.phase == PairingPhase.CONFIRMING)
            if (generation != ask.generation || !pressed || now.deviceId != ask.pairing.deviceId) return@withLock flow.value
            val answer = result?.getOrNull()
            val failure = result?.exceptionOrNull() as? TransportFailure
            when {
                answer != null && !answer.awaitingMac -> commit(session.copy(paired = true, pairing = now.copy(
                    phase = PairingPhase.PAIRED, challenge = answer.challenge, problem = null,
                    macWaitBoundMs = null, awaitingUntil = null, macAsks = 0, nextAskAt = null, lastAskAt = null,
                )))
                (failure != null && !failure.retryable) || ask.final -> {
                    ++generation
                    forget = ask.pairing.apiBase
                    val problem = if (failure != null && !failure.retryable) PROBLEM_MAC_DECLINED else PROBLEM_EXPIRED
                    commit(session.copy(paired = false, threads = emptyList(), selectedThreadId = null,
                        pairing = Pairing(apiBase = now.apiBase, route = now.route, problem = problem)))
                }
                else -> {
                    val challenge = answer?.challenge ?: failure?.challenge ?: now.challenge
                    val next = MacWait.nextAskAt(now.macAsks - 1, ask.at, ports.clock.now(), now.awaitingUntil!!, holds())
                    commit(session.copy(pairing = now.copy(phase = PairingPhase.AWAITING_MAC, challenge = challenge, nextAskAt = next)))
                }
            }
        }
        forget?.let { ports.keys.delete(it) }
        return state
    }

    /**
     * One ask of the wait, when the app's timer says one is due (`mac-wait`). Every rule lives here,
     * so a timer that fires early, late or twice cannot ask more often than [MacWait] allows: one at
     * a time, nothing hidden, nothing before its time, never more than [MacWait.MAX_ASKS]. At or past
     * the deadline it is the LAST ask, with no `Prefer` (Sage's review §2: a press that landed after
     * the previous ask is heard, so no Mac says paired to a phone that threw its key away). The count
     * is written before the request goes, so a restart cannot reset it.
     */
    private suspend fun macWait(): AppState = coroutineScope {
        val operation = currentCoroutineContext()[Job]!!
        var expired: String? = null
        val ask = mutex.withLock {
            val p = session.pairing
            if (p.phase != PairingPhase.AWAITING_MAC || macWaitJob != null) return@withLock null
            val now = ports.clock.now()
            val until = p.awaitingUntil
            val spent = p.macAsks >= MacWait.MAX_ASKS
            if (until == null || p.apiBase == null || p.deviceId == null || p.challenge == null || (spent && now >= until)) {
                // Nothing left to ask with: the Mac has forgotten this key by now, and so does the phone.
                ++generation
                expired = p.apiBase
                commit(session.copy(paired = false, threads = emptyList(), selectedThreadId = null,
                    pairing = Pairing(apiBase = p.apiBase, route = p.route, problem = PROBLEM_EXPIRED)))
                return@withLock null
            }
            val due = dueAt(p)
            if (!visible || due == null || now < due) {
                emit()
                return@withLock null
            }
            val final = now >= until
            val holds = holds()
            macWaitJob = operation
            askOnReturn = false
            val next = p.copy(macAsks = p.macAsks + 1, lastAskAt = now, nextAskAt = now + MacWait.nextDelayMs(p.macAsks, 0, holds))
            commit(session.copy(pairing = next))
            Ask(next, generation, now, final, if (holds && !final) MacWait.preferWaitSeconds(now, until) else 0)
        }
        expired?.let { origin -> if (mutex.withLock { session.pairing.phase == PairingPhase.UNPAIRED }) ports.keys.delete(origin) }
        if (ask == null) return@coroutineScope flow.value
        try {
            // Abandoned because the app left the screen: nothing from it is applied (askMac's ensureActive).
            answered(ask, askMac(ask), operation)
        } finally {
            withContext(NonCancellable) { mutex.withLock { released(operation) } }
        }
    }

    /** An ask that ended without an outcome (abandoned): free the slot and republish, so a return re-arms the timer. */
    private fun released(operation: Job) {
        if (macWaitJob === operation) {
            macWaitJob = null
            emit()
        }
    }

    /**
     * When the next ask may go, or null for none (the ceiling is spent before the deadline). The
     * scheduled time; after a return to the screen, at once, or while the Mac holds no sooner than
     * [MacWait.MIN_SPACING_MS] after the previous ask (`pair_wait`: "coming back asks again").
     */
    private fun dueAt(p: Pairing): Long? {
        val until = p.awaitingUntil ?: return 0L
        if (p.macAsks >= MacWait.MAX_ASKS) return until
        val scheduled = p.nextAskAt ?: 0L
        if (!askOnReturn) return scheduled
        val back = if (holds()) (p.lastAskAt ?: 0L) + MacWait.MIN_SPACING_MS else 0L
        return minOf(scheduled, back)
    }

    /** When the app's timer is next needed by the wait, or null (not waiting, an ask is out, or not on screen). */
    private fun macWaitDue(): Long? {
        val p = session.pairing
        if (!visible || p.phase != PairingPhase.AWAITING_MAC || macWaitJob != null) return null
        val due = dueAt(p) ?: return null
        return maxOf(0L, due - ports.clock.now())
    }

    private suspend fun forget(): AppState = mutex.withLock {
        if (!outbox.isEmpty()) throw CoreError(UNSENT_BEFORE_FORGET)
        val origin = session.pairing.apiBase
        ++generation
        val next = commit(session.copy(paired = false, threads = emptyList(), selectedThreadId = null, pairing = Pairing(),
            sent = emptyList(), echoes = emptyList()))
        origin?.let { ports.keys.delete(it) }
        next
    }

    // --- plumbing ------------------------------------------------------------------------------

    private suspend fun commit(next: Session): AppState {
        ports.session.write(next)
        // Every challenge the phone holds came from the Mac, which minted it moments before (a
        // response header, a `hello`, a pairing answer): its age is counted from here.
        if (next.pairing.challenge != session.pairing.challenge) challengeLearnedAt = next.pairing.challenge?.let { ports.clock.now() }
        session = next
        flow.value = snapshot()
        return flow.value
    }

    /** Publishes the current state without writing the session (outbox-only changes). */
    private fun emit(): AppState {
        flow.value = snapshot()
        return flow.value
    }

    private fun revoked() = session.pairing.problem == "revoked" && !session.paired

    // --- voice (round-12 groups 4, 5, 6; the iOS core's VoiceReducer, via [VoiceMachine]) ----------

    private var voiceSession: VoiceSession? = null
    private var toast: Toast? = null
    private var microphonePrompt = false

    /** The microphone-off card is up (D03); not saved: after a restart only a new press raises it. */
    private var microphoneCard = false

    /** While denied: the system would still show its question. The OS's answer, never saved. */
    private var microphoneCanAsk = false

    private fun voiceWorld() =
        VoiceWorld(voiceSession, session.microphone, session.keptRecordings, toast, microphonePrompt, canRecord(), microphoneCard, microphoneCanAsk)

    private fun canRecord() = session.paired && "voice" in session.capabilities && !unsupported && !session.voicePaused

    // --- notifications, settings, update notices (the iOS core's SettingsReducer, UpdateReducer) ----

    private var sheet: Sheet? = null
    private var focusMessageId: String? = null

    private suspend fun settings(action: Action): AppState {
        // What the Mac is told after the lock is released (network work never holds it), and the
        // key a forgotten pairing leaves behind, deleted only once that message is signed and sent.
        var tellMac: Unregistration? = null
        var forgetKey: String? = null
        var forgot = false
        val state = mutex.withLock { settingsLocked(action, { tellMac = it }, { forgetKey = it }, { forgot = true }) }
        tellMac?.let { unregisterFromMac(it) }
        // Privacy evidence E5: nothing the push provider keeps about this phone outlives the pairing.
        if (forgot) ports.platform.forgetInstallation()
        forgetKey?.let { origin ->
            // Unless this phone started pairing with the same Mac again meanwhile.
            mutex.withLock { if (session.pairing.apiBase != origin) ports.keys.delete(origin) }
        }
        return if (tellMac != null || forgetKey != null || forgot) flow.value else state
    }

    /** One `native_push: null` to send: the pairing to sign it with, and whether it follows Forget. */
    private class Unregistration(val pairing: Pairing, val generation: Int, val forgetting: Boolean)

    private suspend fun settingsLocked(action: Action, tellMac: (Unregistration) -> Unit, forgetKey: (String) -> Unit, forgot: () -> Unit): AppState {
        val n = session.notifications
        return when (action) {
            Action.TurnOnNotifications -> {
                if (!session.paired || n.status == NotificationStatus.ON || n.status == NotificationStatus.TURNING_ON) return emit()
                commit(session.copy(notifications = n.copy(status = NotificationStatus.TURNING_ON))).also { ports.platform.requestNotifications(n.previews) }
            }
            is Action.NotificationsResult -> commit(
                session.copy(notifications = n.copy(status = action.status, offerDismissed = n.offerDismissed || action.status == NotificationStatus.ON)),
            )
            Action.TurnOffNotifications -> {
                if (n.status != NotificationStatus.ON && n.status != NotificationStatus.TURNING_ON) return emit()
                // Privacy evidence E4: the Mac drops this phone's push token now, not at the next
                // reply's failed delivery. Best effort, as the iPhone does.
                if (session.paired) tellMac(Unregistration(session.pairing, generation, forgetting = false))
                commit(session.copy(notifications = n.copy(status = NotificationStatus.OFF))).also { ports.platform.unregisterNotifications() }
            }
            Action.DismissNotificationOffer -> commit(session.copy(notifications = n.copy(offerDismissed = true)))
            is Action.SetPreviews -> {
                if (n.previews == action.on) return emit()
                commit(session.copy(notifications = n.copy(previews = action.on))).also {
                    if (n.status == NotificationStatus.ON) ports.platform.requestNotifications(action.on)
                }
            }
            is Action.OpenSheet -> { sheet = action.sheet; emit() }
            Action.CloseSheet -> { sheet = null; emit() }
            Action.ForgetPairing -> {
                if (session.pairing.phase == PairingPhase.UNPAIRED) return emit()
                sheet = if (outbox.isEmpty()) Sheet.FORGET else Sheet.FORGET_BLOCKED
                emit()
            }
            Action.DiscardAndForget -> {
                if (session.pairing.phase == PairingPhase.UNPAIRED) return emit()
                for (item in outbox.all()) {
                    outbox.discard(item.clientId)
                    item.attachments.orEmpty().forEach { ports.files.delete(it.id) }
                }
                sheet = Sheet.FORGET
                settingsLocked(Action.ConfirmForget, tellMac, forgetKey, forgot)
            }
            Action.ConfirmForget -> {
                if (sheet != Sheet.FORGET || !outbox.isEmpty()) return emit()
                if (n.status == NotificationStatus.ON || n.status == NotificationStatus.TURNING_ON) ports.platform.unregisterNotifications()
                // Privacy evidence E4: whatever the switch says now (off, or denied in Android
                // Settings after it was on), a Mac that takes FCM registrations is told to drop
                // this phone's. Signed with the pairing's key, so the key goes after it.
                if (session.paired && FCM in session.capabilities) tellMac(Unregistration(session.pairing, generation, forgetting = true))
                session.pairing.apiBase?.let(forgetKey)
                forgot()
                ++generation
                sheet = null
                microphoneCard = false
                // Nothing unsent is discarded silently: kept recordings stay, as do the theme and the
                // OS's microphone answer. Everything that belonged to that Mac goes.
                commit(Session(theme = session.theme, keptRecordings = session.keptRecordings, microphone = session.microphone))
            }
            Action.OpenSystemSettings -> { ports.platform.openSystemSettings(); emit() }
            is Action.OpenedFromNotification -> {
                focusMessageId = action.messageId
                val thread = action.threadId?.takeIf { id -> session.threads.any { it.id == id } }
                if (thread != null && thread != session.selectedThreadId) commit(session.copy(selectedThreadId = thread)) else emit()
            }
            is Action.ProvisionalReply -> {
                val held = session.cache[action.threadId].orEmpty()
                val known = session.threads.any { it.id == action.threadId } && held.none { ProvisionalReply.reference(it.id) == action.event || it.id == ProvisionalReply.id(action.event) }
                if (known) commit(session.copy(cache = session.cache + (action.threadId to (held + ProvisionalReply.row(action.threadId, action.event, action.text, held))))) else emit()
            }
            Action.ClearFocus -> { focusMessageId = null; emit() }
            is Action.UpdatePolicy -> commit(session.copy(update = action.notice, voicePaused = action.voicePaused))
            Action.DismissUpdate -> {
                val notice = session.update
                if (notice == null || notice.prominence == UpdateNotice.Prominence.REQUIRED) emit() else commit(session.copy(update = null))
            }
            Action.OpenAppStore -> { ports.platform.openAppStore(); emit() }
            Action.OpenSupport -> { ports.platform.openSupport(); emit() }
            // Updates on Android come only from Google Play: checking is opening the listing.
            Action.CheckForUpdates -> { ports.platform.openAppStore(); emit() }
            Action.OpenPrivacyPolicy -> { ports.platform.openPrivacyPolicy(); emit() }
            else -> emit()
        }
    }

    /** Serializes this phone's push registrations with the Mac, so "off, then on" never lands as "on, then off". */
    private val pushLane = Mutex()

    /**
     * `{"native_push": null}` (contract §7.2, `MacApi.registerPush`), best effort: an unreachable
     * Mac changes nothing here, and the Worker clears the token at the next failed delivery anyway.
     * A turn-off that a turn-on has already overtaken is not sent.
     */
    private suspend fun unregisterFromMac(u: Unregistration) = pushLane.withLock {
        val p = u.pairing
        val apiBase = p.apiBase ?: return@withLock
        val deviceId = p.deviceId ?: return@withLock
        val challenge = p.challenge ?: return@withLock
        if (!u.forgetting && mutex.withLock { generation != u.generation || session.notifications.status != NotificationStatus.OFF }) return@withLock
        val fresh = runCatching { api.registerPush(apiBase, deviceId, challenge, null) }.getOrNull()?.second ?: return@withLock
        if (!u.forgetting) mutex.withLock {
            if (generation == u.generation && session.pairing.apiBase == apiBase) commit(session.copy(pairing = session.pairing.copy(challenge = fresh)))
        }
    }

    private fun voiceItem(id: String, threadId: String, durationMs: Long, levels: List<Double>) =
        voiceItem(id, threadId, id, durationMs, levels)

    private fun voiceItem(id: String, threadId: String, fileId: String, durationMs: Long, levels: List<Double>): OutboxItem {
        val sentAt = isoMillis(ports.clock.now())
        val seconds = durationMs / 1000.0
        return OutboxItem(
            clientId = id, threadId = threadId, kind = "voice", text = "Voice message", queuedAt = sentAt,
            fileId = fileId, codec = "wav16k", sampleRate = 16_000, seconds = seconds, levels = levels,
            wire = Wire.voicePath(id, threadId, seconds, sentAt),
        )
    }

    private suspend fun voice(action: Action): AppState {
        val sent = mutex.withLock { voiceLocked(action) }
        return if (sent) flush() else flow.value
    }

    private suspend fun voiceLocked(action: Action): Boolean {
        var sent = false
        val (next, effects) = VoiceMachine.reduce(voiceWorld(), action, ports.clock.now())
        voiceSession = next.voice
        toast = next.toast
        microphonePrompt = next.microphonePrompt
        microphoneCard = next.microphoneCard
        microphoneCanAsk = next.microphoneCanAsk
        for (effect in effects) {
            when (effect) {
                is VoiceEffect.StartRecording -> try {
                    ports.recorder.stopPlayback()
                    playingRecordingId = null
                    endReply()
                    val journal = session.copy(activeRecording = KeptRecording(effect.id, 0, reason = KeptReason.INTERRUPTED, recordedAt = ports.clock.now()))
                    ports.session.write(journal)
                    session = journal
                    if (!visible) {
                        voiceSession = null
                        commit(session.copy(activeRecording = null))
                        return false
                    }
                    ports.recorder.start(effect.id)
                    if (!visible) return voiceLocked(Action.VoiceInterrupted(ports.clock.now()))
                } catch (failure: Throwable) {
                    voiceSession = null
                    emit()
                    throw failure
                }
                is VoiceEffect.StopRecording -> ports.recorder.stop(effect.id, effect.keep)
                is VoiceEffect.DeleteRecording -> {
                    if (playingRecordingId == effect.id) { ports.recorder.stopPlayback(); playingRecordingId = null }
                    ports.recorder.delete(effect.id)
                }
                VoiceEffect.RequestMicrophone -> ports.recorder.requestMicrophone()
                VoiceEffect.HapticTick -> ports.recorder.haptic()
                is VoiceEffect.Send -> {
                    if (playingRecordingId == effect.id) { ports.recorder.stopPlayback(); playingRecordingId = null }
                    val thread = session.selectedThreadId ?: continue
                    enqueueConsuming(listOf(voiceItem(effect.id, thread, effect.durationMs, effect.levels)),
                        session.copy(microphone = next.microphone, keptRecordings = next.kept, activeRecording = null))
                    sent = true
                }
            }
        }
        val capturing = next.voice?.phase in setOf(VoicePhase.HELD, VoicePhase.LOCKED)
        val active = session.activeRecording.takeIf { capturing }
        if (next.microphone != session.microphone || next.kept != session.keptRecordings || active != session.activeRecording) {
            commit(session.copy(microphone = next.microphone, keptRecordings = next.kept, activeRecording = active))
        } else {
            emit()
        }
        return sent
    }

    /**
     * `client.js` `effectiveConnectionReason`: revoked, then incompatible, then connected, then the
     * link's reason, refined by what the OS says about Tailscale on the Tailscale route (D05).
     */
    private fun effectiveConnection(): ConnectionState = when {
        revoked() -> connection.copy(reason = ConnectionReason.REVOKED)
        unsupported -> connection.copy(reason = ConnectionReason.INCOMPATIBLE)
        session.online -> connection.copy(reason = ConnectionReason.CONNECTED)
        else -> connection.copy(reason = Connections.cause(connection.reason, session.pairing.route, session.paired, vpn))
    }

    /** Whether the OS reports a VPN on the default network; null until it has said (D05). Never probed. */
    @Volatile private var vpn: Boolean? = null

    private fun snapshot() =
        AppState.of(session, (outbox.all() + session.pendingEnqueues).distinctBy { it.clientId }, outbox.dueInMs(), lastSend, Connections.view(effectiveConnection(), ports.clock.now())).copy(
            voice = voiceSession,
            playingRecordingId = playingRecordingId,
            replyPlayback = reply,
            voiceElapsedMs = voiceSession?.takeIf { it.recordingStartedAtMs != null }?.elapsedMs,
            microphone = session.microphone,
            microphonePrompt = microphonePrompt,
            microphoneCard = microphoneCard && session.microphone == Microphone.DENIED,
            microphoneCanAsk = microphoneCanAsk && session.microphone == Microphone.DENIED,
            keptRecordings = session.keptRecordings,
            toast = toast,
            canRecord = canRecord(),
            notifications = session.notifications,
            attachmentLimits = session.attachmentLimits,
            olderAvailable = hasOlder(session.selectedThreadId),
            streamCursor = session.streamCursor,
            pendingAttachments = session.pendingAttachments,
            attachNotice = attachNotice,
            loadingOlder = loadingOlder,
            sheet = sheet,
            focusMessageId = focusMessageId,
            update = session.update,
            voicePaused = session.voicePaused,
            macWaitDueInMs = macWaitDue(),
            fileFetches = fetches.toMap(),
        )

    // --- the connection (connection.js + client.js onState) --------------------------------------

    suspend fun openedWithoutDraining(): AppState = link(LinkStatus.OPEN, drain = false)

    private suspend fun link(status: LinkStatus, drain: Boolean = true): AppState {
        mutex.withLock {
            val now = ports.clock.now()
            connection = if (status == LinkStatus.OPEN) {
                // A link that just opened owes every waiting message a try now ([Outbox.dueNow]):
                // the drain that follows this opening sends them in the same tick.
                outbox.dueNow()
                connection.copy(reason = ConnectionReason.CONNECTED, hasConnected = true, troubleSince = null)
            } else {
                val keep = connection.reason == ConnectionReason.PHONE_OFFLINE || connection.reason == ConnectionReason.SERVICE_UNAVAILABLE
                val reason = when {
                    keep -> connection.reason
                    status == LinkStatus.OPENING && !connection.hasConnected && connection.reason == ConnectionReason.CONNECTING -> ConnectionReason.CONNECTING
                    else -> ConnectionReason.RECONNECTING
                }
                connection.copy(reason = reason, troubleSince = connection.troubleSince ?: now)
            }
            commit(session.copy(online = status == LinkStatus.OPEN))
        }
        return if (drain && status == LinkStatus.OPEN && session.paired) flush() else flow.value
    }

    companion object {
        const val UNSENT_BEFORE_PAIRING = "A message is still waiting for the Mac this phone is paired with. Send it or discard it, then pair."
        /** The Mac capability that says it takes FCM registrations (Echo 65952d16). */
        private const val FCM = "native-push-fcm"

        const val UNSENT_BEFORE_FORGET = "A message is still waiting to be sent. Send it or discard it, then forget this pairing."

        /** Pairing v2's problem codes ([Pairing.problem]). */
        const val PROBLEM_MAC_NEEDS_UPDATE = "mac-needs-update"
        const val PROBLEM_MAC_DECLINED = "mac-declined"
        const val PROBLEM_EXPIRED = "expired"
        /** "They do not match" pressed on the phone (Urban's review, state 4; the iPhone's `wordsRejected`). */
        const val PROBLEM_WORDS_REJECTED = "words-rejected"

        /**
         * One ask of the wait gets this long; no answer in time is waited through like a fault. It
         * must exceed a full hold (`pair_wait.request_timeout_must_exceed_ms`, 14 s), or the phone
         * would cut off the very answer it asked the Mac to hold: 14 s of hold plus 6 s for the
         * round trip. (`HttpsMac`'s own read timeout is 30 s.)
         */
        const val MAC_WAIT_REQUEST_MS = 20_000L

        /**
         * How long the press may take before the six words give way to "Now press They match on
         * your Mac" (`app.js` `SHOW_WAITING_AFTER_MS`): an answer inside it (the Mac was pressed
         * first, or a Mac that does not hold) goes straight to its own screen, with no flash.
         */
        const val SHOW_WAITING_AFTER_MS = 500L

        suspend fun open(ports: Ports): RichCore {
            val outbox = Outbox(ports.storage, ports.clock)
            outbox.load()
            return RichCore(ports, ports.session.read(), outbox).also { core ->
                core.finishEnqueues()
                core.session.activeRecording?.let { interrupted ->
                    val known = core.session.keptRecordings.any { it.id == interrupted.id } || outbox.all().any { it.clientId == interrupted.id }
                    val recovered = if (known) null else ports.recorder.recover(interrupted)
                    core.commit(core.session.copy(activeRecording = null,
                        keptRecordings = core.session.keptRecordings + listOfNotNull(recovered)))
                }
                // A pairing the last process left part-way. An exchange's answer died with that
                // process, and six words from a build before pairing v2 are the old words, which a v2
                // phone never shows: both say pairing did not finish, and a fresh code starts again.
                // A wait for the press on the Mac (or a press the last process left on the wire) carries
                // on and asks again at once (the timer is armed by the state published here); past
                // its deadline that ask is the last one (`pair_wait`: "A relaunch that finds the
                // deadline already passed asks once too").
                val p = core.session.pairing
                val pressed = p.phase == PairingPhase.CONFIRMING && p.awaitingUntil != null
                val unfinished = p.phase == PairingPhase.EXCHANGING || (p.phase == PairingPhase.CONFIRMING && p.macWaitBoundMs == null && !pressed)
                core.askOnReturn = pressed || p.phase == PairingPhase.AWAITING_MAC
                if (unfinished) {
                    core.commit(core.session.copy(paired = false, pairing = Pairing(apiBase = p.apiBase, route = p.route, problem = "fault")))
                    if (p.phase == PairingPhase.CONFIRMING) p.apiBase?.let { ports.keys.delete(it) }
                } else if (pressed) {
                    core.commit(core.session.copy(pairing = p.copy(phase = PairingPhase.AWAITING_MAC)))
                } else {
                    core.emit()
                }
            }
        }

        /** The route a pairing origin names (contract §1.1), or null for any other host. */
        fun routeOf(origin: String): Route? {
            val host = runCatching { URI(origin).host?.lowercase() }.getOrNull() ?: return null
            return when {
                host.endsWith(".ts.net") -> Route.TAILNET
                host.endsWith(".richos.ceo") -> Route.CONNECT
                else -> null
            }
        }

        fun actionName(action: Action): String = when (action) {
            Action.PlayKept -> "play-kept"
            is Action.PlaybackEnded -> "playback-ended"
            is Action.HearReply -> "reply-play"
            is Action.StopReply -> "reply-stop"
            is Action.OpenRichFile -> "file-open"
            is Action.StopRichFile -> "file-stop"
            is Action.RichFileViewed -> "file-viewed"
            is Action.SelectThread -> "select-thread"
            is Action.RememberReading -> "remember-reading"
            is Action.Compose -> "compose"
            is Action.AnswerQuestion -> "answer-question"
            Action.Send -> "send"
            is Action.SendVoice -> "send-voice"
            is Action.Network -> "network"
            Action.Retry -> "retry"
            Action.Sync -> "sync"
            is Action.Discard -> "discard"
            is Action.SetTheme -> "theme"
            is Action.Pair -> "pair"
            is Action.ConfirmWords -> "confirm-words"
            Action.Forget -> "forget"
            is Action.Receive -> "receive"
            is Action.Link -> "link"
            is Action.Health -> "health"
            Action.Tick -> "tick"
            is Action.VoicePress -> "voice-press"
            is Action.VoiceStartLocked -> "voice-start-locked"
            is Action.MicrophonePermission -> "microphone-permission"
            Action.AskMicrophone -> "ask-microphone"
            Action.DismissMicrophoneCard -> "dismiss-microphone-card"
            is Action.VoiceMove -> "voice-move"
            is Action.VoiceRelease -> "voice-release"
            is Action.VoiceLockedSend -> "voice-locked-send"
            is Action.VoiceLockedCancel -> "voice-locked-cancel"
            is Action.VoiceTouchCanceled -> "voice-touch-canceled"
            is Action.VoiceInterrupted -> "voice-interrupted"
            is Action.VoiceLevel -> "voice-level"
            Action.VoiceSettled -> "voice-settled"
            is Action.SendKept -> "send-kept"
            is Action.DiscardKept -> "discard-kept"
            else -> action::class.simpleName ?: "action"
        }
    }
}
