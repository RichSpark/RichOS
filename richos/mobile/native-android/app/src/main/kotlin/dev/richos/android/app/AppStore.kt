package dev.richos.android.app

import dev.richos.android.core.Action
import dev.richos.android.core.AppState
import dev.richos.android.core.CoreError
import dev.richos.android.core.FileFetchState
import dev.richos.android.core.RichCore
import dev.richos.android.core.VoicePhase
import dev.richos.android.core.VoiceSession
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * The app's single owner of the core (build plan §3.3 "an AppStore in :app exposes
 * StateFlow<AppState>"). Screens collect [states] and call [dispatch]; they never hold a
 * core themselves, so the debug bridge can swap in the development runtime's core and the
 * screen follows it without knowing.
 *
 * [scope] must run on the main thread: actions are launched in the order the user made
 * them, and the core's lock keeps that order through every suspension.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class AppStore(private val scope: CoroutineScope) {
    private val core = MutableStateFlow<RichCore?>(null)
    private val refusal = MutableStateFlow<String?>(null)
    private var storageFailed = false

    /** Null until the core has opened its saved state; then every committed state. */
    val states: StateFlow<AppState?> = core
        .flatMapLatest { it?.states ?: flowOf(null) }
        .stateIn(scope, SharingStarted.Eagerly, null)

    /** The sentence from the last refused action, for a screen that wants to show it. */
    val lastRefusal: StateFlow<String?> = refusal.asStateFlow()

    val current: RichCore? get() = core.value

    init {
        // THE APP'S ONE TIMER (queue.js rule 5; the core sets none): while a recording runs, a tick
        // every 100 ms drives the press delay, the timer and the ceiling; otherwise one tick when
        // the reconnecting notice falls due. And the outbox: when a waiting message's back-off
        // runs out, a drain (`sync`) sends what is owed, so a failed send is retried with nobody
        // pressing Try now (the PWA's `scheduleOutboxDrain`, app.js). Every new state restarts the
        // wait, so there is only ever one.
        //
        // Nothing else wakes it: an idle screen gets no state and so draws no frame. A recording that
        // has ENDED is not running: its end plays on the screen's own clock, and a tick then would
        // only restamp the time, a new state ten times a second for as long as the ending stayed
        // unsettled (measured 2026-09-24: forever after a tap on the microphone, AppStoreIdleTest).
        //
        // And pairing v2's wait for the press on the Mac: core publishes when its next ask (or its
        // bound) falls due, only while the app is on screen, and `mac-wait` lets core decide. A
        // hidden app publishes no due time, so this timer holds nothing for it (CEO ruling §81).
        scope.launch {
            states.collectLatest { s ->
                if (s == null) return@collectLatest
                val tick = if (s.voice?.let(::running) == true) VOICE_TICK_MS else s.connection.noticeDueInMs
                val owed = outboxOwedInMs(s)
                val mac = s.macWaitDueInMs
                val due = listOfNotNull(tick, owed, mac).minOrNull() ?: return@collectLatest
                delay(due)
                if (tick != null && tick <= due) dispatch(Action.Tick)
                if (owed != null && owed <= due) drain()
                if (mac != null && mac <= due) dispatch(Action.MacWait)
            }
        }
        // A file Rich attached that was tapped while the Mac was out of reach ("Opens when your Mac
        // is back online", round 36): the tap is asked again once each time the link opens. Core
        // keeps the rule (a second ask while it is coming over does nothing); this only says when.
        scope.launch {
            val asked = mutableSetOf<String>()
            states.collect { s ->
                if (s == null) return@collect
                if (!s.online || !s.paired) {
                    asked.clear()
                    return@collect
                }
                for (fetch in s.fileFetches.values) {
                    if (fetch.state == FileFetchState.WAITING && asked.add(fetch.id)) dispatch(Action.OpenRichFile(fetch.id))
                }
            }
        }
    }

    /**
     * When the outbox is owed a try: only while paired and the link to the Mac is open. Offline, no
     * attempt is spent on unchanged conditions; the wakeup is the link opening, whose own drain
     * sends what is owed then (T3's connection runtime, via the PWA's `online` handler).
     */
    private fun outboxOwedInMs(s: AppState): Long? = if (s.paired && s.online && !storageFailed) s.dueInMs else null

    /** True while a timer drain is running: one at a time, however many states arrive meanwhile. */
    private var draining = false

    /**
     * The outbox drain: `sync`, which sends every message owed a try, oldest first, WITHOUT Try
     * now's reset of the back-off. Also the action for a platform wakeup (the network came back).
     */
    private fun drain() {
        if (draining || storageFailed) return
        val target = core.value ?: return
        draining = true
        scope.launch {
            try {
                target.dispatch(Action.Sync)
            } catch (e: CoreError) {
                refusal.value = e.message
            } catch (_: IOException) {
                storageFailed = true
                refusal.value = "Could not save your changes. Your work has been kept; free storage and try again."
            } finally {
                draining = false
            }
        }
    }

    /** Opens the production core, unless something (the debug bridge) installed one first. */
    fun open(factory: suspend () -> RichCore) {
        scope.launch {
            try {
                val opened = factory()
                if (core.value == null) core.value = opened
            } catch (e: IOException) {
                storageFailed = true
                refusal.value = e.message ?: "Saved data could not be read. It has been preserved. Contact support."
            }
        }
    }

    /** Replaces the core. Debug builds only: the development bridge installs its runtime's core. */
    fun install(replacement: RichCore) {
        core.value = replacement
    }

    fun reportStorageFailure() {
        storageFailed = true
        refusal.value = "Could not save your changes. Your work has been kept; free storage and try again."
    }

    fun dispatch(action: Action) = dispatchThen(action, null)

    /**
     * [dispatch], then [done] with core's refusal sentence, or null when core took the action. For a
     * screen whose next step depends on the answer (the pairing link sheet closes only once core
     * has taken the link). Before the core has opened nothing is dispatched and nothing is reported.
     */
    fun dispatchThen(action: Action, done: ((refusal: String?) -> Unit)?) {
        val target = core.value ?: return
        if (action == Action.Send) PerformanceMarks.mark("send-tapped")
        scope.launch {
            val refused = try {
                target.dispatch(action)
                storageFailed = false
                refusal.value = null
                null
            } catch (e: CoreError) {
                refusal.value = e.message
                e.message ?: "refused"
            } catch (_: IOException) {
                storageFailed = true
                val message = "Could not save your changes. Your work has been kept; free storage and try again."
                refusal.value = message
                message
            }
            done?.invoke(refused)
        }
    }

    /** Core's current draft, including an edit retained in memory after a reported save failure. */
    val currentDraft: String get() = core.value?.state?.draft ?: ""

    /**
     * The composer's write of its draft: `compose`, in order with every other action, then [done] on
     * the main thread once core has committed or refused it (at once when no core is open yet).
     */
    fun composeDraft(text: String, done: () -> Unit) {
        if (core.value == null) return done()
        dispatchThen(Action.Compose(text)) { done() }
    }

    companion object {
        const val VOICE_TICK_MS = 100L

        /** A recording the core still needs time for: the press delay, the timer, the ceiling. */
        fun running(voice: VoiceSession): Boolean = voice.phase != VoicePhase.ENDING
    }
}
