package dev.richos.android.ui

import androidx.compose.foundation.border
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.semantics.Role
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTag
import androidx.compose.ui.unit.dp
import dev.richos.android.core.AppState
import dev.richos.android.design.EdgeCuedRegion
import dev.richos.android.design.Rich
import dev.richos.android.design.RichTheme
import dev.richos.android.design.lamp
import dev.richos.android.design.speckle
import dev.richos.android.ui.composer.Composer
import dev.richos.android.ui.composer.ComposerCardView
import dev.richos.android.ui.composer.InlineNoticeView
import dev.richos.android.ui.composer.RecoveryCard
import dev.richos.android.ui.composer.WaitingToSendCard
import dev.richos.android.ui.composer.NotificationOfferCard
import dev.richos.android.core.Sheet
import dev.richos.android.core.VoiceEnding
import dev.richos.android.core.VoicePhase as CorePhase
import dev.richos.android.design.RichMotion
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import dev.richos.android.ui.attach.AttachMenu
import dev.richos.android.ui.attach.DeniedCard
import dev.richos.android.ui.attach.MacOffCard
import dev.richos.android.ui.attach.PhotoViewer
import dev.richos.android.ui.attach.RichFileViewer
import dev.richos.android.ui.attach.RejectionCard
import dev.richos.android.ui.attach.ShareSheetView
import dev.richos.android.ui.attach.SystemPickerDrawing
import dev.richos.android.ui.model.Viewer
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.ui.semantics.contentDescription
import dev.richos.android.ui.conversation.EmptyConversation
import dev.richos.android.ui.conversation.FollowThreshold
import dev.richos.android.ui.conversation.Header
import dev.richos.android.ui.conversation.HeaderFade
import dev.richos.android.ui.conversation.LatestPill
import dev.richos.android.ui.conversation.OutOfReachLine
import dev.richos.android.ui.conversation.Thread
import dev.richos.android.ui.conversation.distanceFromNewest
import dev.richos.android.ui.conversation.rememberThreadController
import dev.richos.android.ui.model.HistoryEdge
import dev.richos.android.ui.model.Overlay
import dev.richos.android.ui.model.PairingStep
import dev.richos.android.ui.model.ScreenModel
import dev.richos.android.ui.model.ScrollPose
import dev.richos.android.ui.model.UpdateNotice
import dev.richos.android.ui.pairing.PairingEntryOverlays
import dev.richos.android.ui.overlays.Consent
import dev.richos.android.ui.overlays.ForgetBlockedDialog
import dev.richos.android.ui.overlays.ForgetDialog
import dev.richos.android.ui.overlays.KeyboardDrawing
import dev.richos.android.ui.overlays.MicrophonePermissionDrawing
import dev.richos.android.ui.overlays.NeedsNewerApp
import dev.richos.android.ui.overlays.PairingBlockedDialog
import dev.richos.android.ui.overlays.PairingIntro
import dev.richos.android.ui.overlays.PairingProgress
import dev.richos.android.ui.overlays.RemovedFromMac
import dev.richos.android.ui.overlays.WaitingForMac
import dev.richos.android.ui.overlays.Scanner
import dev.richos.android.ui.overlays.Scrim
import dev.richos.android.ui.overlays.SettingsSheet
import dev.richos.android.ui.overlays.ShadeFrame
import dev.richos.android.ui.overlays.SixWords
import dev.richos.android.ui.overlays.UpdateBanner
import dev.richos.android.ui.overlays.UpdateDialog
import dev.richos.android.ui.overlays.UpdateRequired
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The whole app, drawn from core's [AppState] alone: every stand-in empty. This is what the
 * activity shows today; each screen group lights up as core grows the state it needs.
 */
@Composable
fun RichApp(state: AppState, onEvent: (UiEvent) -> Unit, camera: (@Composable () -> Unit)? = null) =
    RichApp(ScreenModel(app = state), onEvent, camera)

/**
 * The whole app for one [ScreenModel]. Pure: the same model always draws the same frame, so every
 * screen and state is reachable by name from a fixture (see `ScreenCatalog`) and renders headless.
 *
 * The only state kept here decides nothing: whether the + menu and the photo viewer are open, the
 * scroll position the reader owns, and the clock of a transition the screen plays after core ends
 * a recording (which it then reports as `voice-settled`).
 */
@Composable
fun RichApp(model: ScreenModel, onEvent: (UiEvent) -> Unit, camera: (@Composable () -> Unit)? = null) {
    RichTheme(model.app.theme) {
        // Navigation that decides nothing: the + menu and the viewer.
        var menuOpen by remember(model.attach.menuOpen) { mutableStateOf(model.attach.menuOpen) }
        var viewer by remember(model.viewer) { mutableStateOf(model.viewer) }
        val focusManager = LocalFocusManager.current
        val handle: (UiEvent) -> Unit = { e ->
            when (e) {
                // The composer must not keep its keyboard over the Settings sheet.
                UiEvent.OpenSettings -> focusManager.clearFocus()
                // Nor over a file's viewer: with it up, the viewer opened squeezed above it (Honor).
                is UiEvent.OpenRichFile -> focusManager.clearFocus()
                // A Mac that takes no photos or files: the + says why (core's card) instead of a menu.
                UiEvent.AttachMenu -> if (model.attachmentsSupported) menuOpen = !menuOpen else onEvent(UiEvent.AttachPick("photos"))
                is UiEvent.AttachPick -> menuOpen = false
                is UiEvent.OpenViewer -> viewer = Viewer(e.messageId, e.index)
                UiEvent.CloseViewer -> viewer = null
                else -> Unit
            }
            onEvent(e)
        }
        val shown = withVoiceClock(model, handle)
        val c = Rich.colors
        // The ground, the design system's speckled ground over it, then the lamp (design/Speckle.kt).
        Box(Modifier.fillMaxSize().background(c.ground).speckle().lamp().semantics { testTag = "app" }) {
            val step = model.pairingStep
            val scanning = step == PairingStep.SCANNING || step == PairingStep.FOUND
            when {
                // The scanner is full screen over whatever pairing screen opened it, "removed from
                // your Mac" included ("Pair again" opens it).
                scanning && model.update !is UpdateNotice.Required ->
                    Scanner(found = step == PairingStep.FOUND, onEvent = handle, camera = camera, status = model.scannerCamera)
                model.removedFromMac -> {
                    RemovedFromMac(handle)
                    PairingEntryOverlays(model, handle)
                }
                model.update is UpdateNotice.Required -> UpdateRequired((model.update as UpdateNotice.Required).version, handle)
                step == PairingStep.INTRO -> {
                    PairingIntro(problem = null, onEvent = handle)
                    PairingEntryOverlays(model, handle)
                }
                step == PairingStep.REFUSED || step == PairingStep.PROBLEM -> {
                    PairingIntro(problem = model.app.pairing.problem, onEvent = handle)
                    PairingEntryOverlays(model, handle)
                }
                step == PairingStep.CAMERA_DENIED -> {
                    PairingIntro(problem = model.app.pairing.problem, onEvent = handle)
                    PairingEntryOverlays(model, handle)
                }
                step == PairingStep.IN_PROGRESS -> PairingProgress()
                step == PairingStep.WORDS -> SixWords(model.app.pairing.words, handle)
                step == PairingStep.WAITING_FOR_MAC -> WaitingForMac(model.app.pairing.words, handle)
                step == PairingStep.NEEDS_NEWER_APP -> NeedsNewerApp(handle)
                step == PairingStep.CONSENT || model.sheet == Sheet.WHERE_MESSAGES_GO -> Consent(handle)
                model.shade != null -> ShadeFrame(model.shade.preview)
                model.share != null -> ShareSheetView(model.share, model.attachLimitMb, handle)
                model.picker != null -> SystemPickerDrawing(model.picker)
                else -> {
                    Conversation(shown, menuOpen, handle)
                    Overlays(model, handle)
                    viewer?.let { v -> model.thread.firstOrNull { it.id == v.messageId }?.let { PhotoViewer(it, v.index, handle) } }
                    // A file Rich attached, once it is on this phone and was tapped (round 36 "Phone: opened").
                    model.fileViewer?.let { (file, fetch, time) -> RichFileViewer(file, fetch, time, handle) }
                }
            }
        }
    }
}

@Composable
private fun Overlays(model: ScreenModel, onEvent: (UiEvent) -> Unit) {
    val close = { onEvent(UiEvent.CloseOverlay) }
    when {
        model.forgetBlocked != null -> { Scrim(close); ForgetBlockedDialog(model.forgetBlocked!!, onEvent) }
        model.sheet == Sheet.FORGET -> { Scrim(close); ForgetDialog(onEvent) }
        model.sheet == Sheet.SETTINGS -> {
            Scrim(close)
            SettingsSheet(model.settings, model.notificationStatus, model.app.notifications.previews, onEvent)
        }
        model.pairingBlocked != null -> { Scrim(close); PairingBlockedDialog(model.pairingBlocked!!, onEvent) }
        model.overlay == Overlay.MicrophonePermission -> MicrophonePermissionDrawing()
        model.update is UpdateNotice.Dialog -> { Scrim { onEvent(UiEvent.UpdateLater) }; UpdateDialog((model.update as UpdateNotice.Dialog).line, onEvent) }
    }
}

/**
 * Plays the transitions core's voice state asks for: when a recording ends (sent, canceled) the
 * screen runs its clock through the ritual and then tells core (`voice-settled`); when the phase
 * turns locked it plays the lock transition. A review frame's own [ScreenModel.voiceMomentMs] wins.
 */
@Composable
private fun withVoiceClock(model: ScreenModel, onEvent: (UiEvent) -> Unit): ScreenModel {
    if (model.voiceMomentMs != null) return model
    val shown = withTooShortLine(model)
    val v = model.app.voice ?: return shown
    // EVERY ending is settled, as the iPhone's `VoiceClock.endDurationMs` settles each: an ending
    // left unsettled keeps core's recording alive, so the microphone refuses the next press.
    val ending = v.phase == CorePhase.ENDING
    val locking = v.phase == CorePhase.LOCKED
    val key = "${v.id}:${v.phase}:${v.ending}"
    val clock = remember(key) { Animatable(0f) }
    val length = when {
        ending && v.ending == VoiceEnding.SENT -> RichMotion.SEND_IDLE_AT
        ending && v.ending == VoiceEnding.TOO_SHORT -> TOO_SHORT_SETTLE_MS
        ending && v.ending == VoiceEnding.CEILING -> CEILING_SETTLE_MS
        ending && v.wasLocked -> RichMotion.LOCKED_CANCEL_IDLE_AT
        ending -> RichMotion.BIN_IDLE_AT
        locking -> 450
        else -> 0
    }
    // Too short and the ceiling draw no transition (ScreenModel.voice: the circle is already back,
    // the line or the card explains), so they wait without drawing a frame.
    val drawn = !(ending && (v.ending == VoiceEnding.TOO_SHORT || v.ending == VoiceEnding.CEILING))
    LaunchedEffect(key) {
        if (length > 0) {
            if (drawn) clock.animateTo(length.toFloat(), tween(length, easing = LinearEasing)) else delay(length.toLong())
            if (ending) onEvent(UiEvent.VoiceSettled)
        }
    }
    return when {
        ending && drawn -> shown.copy(voiceMomentMs = clock.value.toInt())
        locking && clock.value < length -> shown.copy(voiceMomentMs = clock.value.toInt())
        else -> shown
    }
}

/** The iPhone's settle times for the endings that draw nothing (`VoiceClock.endDurationMs`). */
private const val TOO_SHORT_SETTLE_MS = 150
private const val CEILING_SETTLE_MS = 200

/**
 * "Hold the button while you speak." stays its full [RichMotion.TOO_SHORT_LINE_MS] (round 12:
 * one calm line, then it goes) although core clears its toast when the ending settles, 150 ms in,
 * so the microphone is free again at once. A new recording takes its place. Nothing here decides:
 * it only keeps a line on screen that core already said.
 */
@Composable
private fun withTooShortLine(model: ScreenModel): ScreenModel {
    val v = model.app.voice
    val tooShort = v?.takeIf { it.phase == CorePhase.ENDING && it.ending == VoiceEnding.TOO_SHORT }?.id
    var line by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(tooShort) { if (tooShort != null) line = tooShort }
    LaunchedEffect(line) {
        if (line != null) {
            delay(RichMotion.TOO_SHORT_LINE_MS.toLong())
            line = null
        }
    }
    val showing = line != null && (v == null || v.id == line) && model.app.toast == null && model.localNotice == null
    return if (showing) model.copy(localNotice = dev.richos.android.ui.model.InlineNotice.TooShort) else model
}

/** The conversation screen: header, thread, Latest pill, and the composer zone above the keyboard. */
@Composable
private fun Conversation(model: ScreenModel, menuOpen: Boolean, onEvent: (UiEvent) -> Unit) {
    val density = LocalDensity.current
    val projected = remember(model.app.messages, model.app.outbox, model.app.sent, model.app.echoes, model.app.selectedThreadId,
        model.voiceMs, model.replyAudio, model.playing, model.zone, (model.nowMs ?: System.currentTimeMillis()) / 60_000,
        model.extra, model.extraAfter, model.focusedId, model.app.online, model.app.replyPlayback, model.app.capabilities) { model.thread }
    // (`online`: a reply still arriving is drawn arriving only while its stream is open, D02.
    // `replyPlayback`, `capabilities`: "Hear it", preparing and playing follow core, and a Mac that
    // offers audio shows "Hear it"; without them the row kept its last drawing.)
    val savedReading = model.app.readingAnchor
    val savedIndex = savedReading?.let { a -> projected.asReversed().indexOfFirst { it.id == a.messageId } } ?: -1
    val controller = rememberThreadController(startFollowing = savedReading == null && model.scroll == ScrollPose.FOLLOWING,
        initialIndex = savedIndex.coerceAtLeast(0), initialOffset = if (savedIndex >= 0) savedReading?.offset ?: 0 else 0)
    val scope = rememberCoroutineScope()
    var headerPx by remember { mutableIntStateOf(0) }
    var zonePx by remember { mutableIntStateOf(0) }
    // The composer's own row and the drawn keyboard: measured so the notes region above them can
    // give way to the header instead of drawing over it (D05, Honor, 2026-09-28). Neither depends
    // on the notes region's own height, so this settles in the next frame, same as headerPx/zonePx.
    var composerRowPx by remember { mutableIntStateOf(0) }
    var keyboardPx by remember { mutableIntStateOf(0) }
    // A reference chip jumps back to what was sent, which glows once (attachments NOTES).
    var glowId by remember { mutableStateOf<String?>(null) }
    val thread = remember(projected, glowId) { if (glowId == null) projected else projected.map { if (it.id == glowId) it.copy(focused = true) else it } }
    val sent = {
        controller.follow.sent()
        onEvent(UiEvent.Reading(null))
        scope.launch { if (thread.isNotEmpty()) controller.list.animateScrollToItem(0) }
        Unit
    }
    val forward: (UiEvent) -> Unit = { e ->
        if (e is UiEvent.RecordingSend) sent()
        if (e is UiEvent.OpenReference) {
            val i = controller.indexOf(e.messageId)
            if (i >= 0) scope.launch {
                controller.follow.dragStarted()
                controller.list.animateScrollToItem(i)
                controller.follow.settled(Float.MAX_VALUE, 0f)
                kotlinx.coroutines.delay(300)
                glowId = e.messageId
            }
        }
        onEvent(e)
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val headerDp = with(density) { headerPx.toDp() }
        val zoneDp = with(density) { zonePx.toDp() }
        val composerRowDp = with(density) { composerRowPx.toDp() }
        val keyboardDp = with(density) { keyboardPx.toDp() }
        // The header keeps its whole height and the composer keeps its place (D05, I02 in spirit):
        // the notes region above the composer gives way, capped to whatever room is actually left
        // below the header once the composer's own row and a drawn keyboard have theirs. Before the
        // first measurement (headerPx/composerRowPx/keyboardPx still 0) this is the old flat 40%,
        // settling to the header-aware bound the next frame, same lag already tolerated for headerPx
        // and zonePx below.
        // The conversation keeps CardsRoom.ConversationMinimum above the cards whatever their number.
        val maxAbove = CardsRoom.maxHeight(maxHeight, headerDp, composerRowDp, keyboardDp)
        if (thread.isEmpty() && model.historyEdge != HistoryEdge.LOADING_OLDER) {
            // Bottom-padded by the composer zone's own measured height (as Thread's scroll is,
            // line below): at the smallest phone and the largest text the added voice-message
            // paragraph is tall enough to reach the composer, and must scroll clear of it rather
            // than sit behind it (ScreensTest conv-empty--*-small-font200). It starts where round
            // 12.1 starts it, "higher so the paragraph clears the composer on both phones": the
            // thread's own top, then 24% of the thread's width (12% on the small phone,
            // `.beginning.empty-first`).
            val lift = (maxWidth - 24.dp) * (if (maxHeight < 700.dp) 0.12f else 0.24f)
            EmptyConversation(
                Modifier.align(Alignment.TopCenter).padding(top = headerDp + 16.dp + lift).padding(bottom = zoneDp + 20.dp),
            )
        } else {
            Thread(
                messages = thread,
                edge = model.historyEdge,
                // The out-of-reach line is not a row: it is pinned under the header (I02, below).
                dayLabel = "Today",
                controller = controller,
                topPadding = headerDp + 16.dp,
                bottomPadding = zoneDp + 20.dp,
                onEvent = forward,
            )
            HeaderFade(headerDp, maxHeight)
        }
        // The attach menu dims the conversation to 55%; the composer stays lit (attachments NOTES).
        if (menuOpen) {
            val scrim = Rich.colors.scrim
            Box(
                Modifier.fillMaxSize().background(scrim.copy(alpha = scrim.alpha * 0.55f / 0.66f))
                    .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClickLabel = "Close") { onEvent(UiEvent.AttachMenu) }
                    .semantics { contentDescription = "Close the attach menu" },
            )
        }
        // Pose a review frame: the reader has scrolled to older history, or to the very top.
        LaunchedEffect(model.scroll, thread.size) {
            when (model.scroll) {
                ScrollPose.TOP -> controller.list.scrollToItem(thread.size + 1)
                ScrollPose.READING_OLDER -> controller.list.scrollToItem((thread.size / 2).coerceAtLeast(1))
                ScrollPose.FOLLOWING -> Unit
            }
        }
        Column(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.statusBars).onSizeChanged { headerPx = it.height }) {
            Header(model.notice, onEvent)
            // Part of the header's measured block, so the thread starts below it and fades under it,
            // never over it (I02).
            if (model.cachedWhileOffline && thread.isNotEmpty()) OutOfReachLine()
            val banner = model.update as? UpdateNotice.Banner
            if (banner != null) UpdateBanner(banner.version, banner.line, onEvent, Modifier.padding(top = 8.dp))
        }
        val distance by remember { androidx.compose.runtime.derivedStateOf { distanceFromNewest(controller.list) } }
        val thresholdPx = with(density) { FollowThreshold.toPx() }
        // The Latest pill floats 12 dp above the composer zone (`.latest`), outside it, so showing
        // it never changes the thread's padding.
        LatestPill(
            visible = thread.isNotEmpty() && controller.follow.showsLatest(distance, thresholdPx),
            onClick = {
                controller.follow.latestTapped()
                onEvent(UiEvent.Reading(null))
                scope.launch { controller.list.animateScrollToItem(0) }
            },
            modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = zoneDp + 8.dp),
        )
        // The composer zone, measured WITH the keyboard and navigation insets below it, so the thread
        // ends above whatever is under the composer: the conversation keeps following (comp-keyboard).
        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth()
                .onSizeChanged { zonePx = it.height }
                .then(if (model.keyboardDrawn) Modifier else Modifier.windowInsetsPadding(WindowInsets.navigationBars.union(WindowInsets.ime))),
        ) {
            Column(Modifier.fillMaxWidth().padding(start = 8.dp, end = 8.dp, bottom = 8.dp)) {
                // Notes above the composer scroll inside a bounded height; the compose row never
                // leaves the screen (iOS audit F1, F11).
                val openQuestions = thread.filter { (it.body as? dev.richos.android.ui.model.Body.Question)?.let { q -> q.card.state == "open" && q.savedAnswer == null } == true }
                if (openQuestions.isNotEmpty()) {
                    BasicText("${openQuestions.size} question${if (openQuestions.size == 1) "" else "s"} open", style = Rich.type.read.copy(color = Rich.colors.ink), modifier = Modifier.align(Alignment.CenterHorizontally).background(Rich.colors.surface, CircleShape)
                        .border(1.dp, if (model.app.theme == dev.richos.android.core.Theme.DARK) Rich.colors.ink.copy(alpha = 0.46f) else Rich.colors.line, CircleShape)
                        .heightIn(min = 48.dp).clickable(role = Role.Button) {
                        val index = controller.indexOf(openQuestions.first().id)
                        if (index >= 0) scope.launch {
                            controller.follow.dragStarted()
                            controller.list.animateScrollToItem(index)
                            controller.follow.settled(Float.MAX_VALUE, 0f)
                        }
                    }.padding(12.dp))
                }
                val recovered = model.app.outbox.filter { it.questionId != null && it.state == dev.richos.android.core.OutboxState.BLOCKED }
                if (recovered.isNotEmpty()) {
                    Column(Modifier.heightIn(max = maxAbove / 2).verticalScroll(rememberScrollState())) {
                        recovered.forEach { answer -> SelectionContainer { BasicText((answer.lastReason ?: "This answer was not sent.") + "\n" + answer.text, style = Rich.type.read.copy(color = Rich.colors.ink)) } }
                    }
                }
                val waiting = model.waitingToSend
                val a = model.attach
                val hasAbove = model.inlineNotice != null || waiting != null || model.cards.isNotEmpty() || model.keptRecording != null || model.notificationOffer ||
                    a.rejection != null || a.denied != null || a.macOffCard
                if (hasAbove) {
                    // The notes showing when the region opens start at its top. A note raised while it
                    // is up is the answer to something just done (a press with the microphone off, a
                    // failed send), so it is brought into view with the least scroll that shows it (I05).
                    val opened = remember { BooleanArray(1) }
                    SideEffect { opened[0] = true }
                    val up = opened[0]
                    EdgeCuedRegion(
                        maxHeight = maxAbove,
                        contentPadding = PaddingValues(start = 2.dp, end = 2.dp, bottom = 8.dp),
                        verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(8.dp),
                    ) {
                        model.inlineNotice?.let { n -> Note("inline:${n::class.simpleName}", up) { InlineNoticeView(n) } }
                        if (waiting != null) Note("waiting", up) { WaitingToSendCard(waiting, onEvent) }
                        model.cards.forEach { card -> Note("card:${card::class.simpleName}", up) { ComposerCardView(card, onEvent) } }
                        if (model.notificationOffer) Note("notifications", up) { NotificationOfferCard(onEvent) }
                        model.keptRecording?.let { k -> Note("kept", up) { RecoveryCard(k, forward) } }
                        a.rejection?.let { r -> Note("rejection", up) { RejectionCard(r, model.attachLimitMb, onEvent) } }
                        a.denied?.let { d -> Note("denied", up) { DeniedCard(d, onEvent) } }
                        if (a.macOffCard) Note("mac-off", up) { MacOffCard(onEvent) }
                    }
                }
                Composer(
                    draft = model.app.draft,
                    voice = model.voice,
                    disabledReason = model.composerDisabledReason,
                    voiceAvailable = model.voiceAvailable,
                    onEvent = onEvent,
                    onSent = sent,
                    modifier = Modifier.onSizeChanged { composerRowPx = it.height },
                    pending = model.attach.pending,
                    menuOpen = menuOpen,
                )
            }
            if (model.keyboardDrawn) KeyboardDrawing(Modifier.onSizeChanged { keyboardPx = it.height })
        }
        if (menuOpen) AttachMenu(onEvent, Modifier.align(Alignment.BottomStart).padding(start = 10.dp, bottom = zoneDp + 6.dp))
    }
}

/**
 * One note in the capped, scrolling region above the composer. Raised while the region was already
 * up ([raisedWhileUp]), it asks its scrolling parent, once, to bring it into view: the least scroll
 * that shows it whole, or its top when it is taller than the region (Compose's default
 * bring-into-view). iPhone parity: isaac-opus-ux2 `94d47006`, `ScrollViewReader.scrollTo` (I05).
 * One bounded scroll per raised note; nothing repeats.
 */
@Composable
private fun Note(id: String, raisedWhileUp: Boolean, content: @Composable () -> Unit) = key(id) {
    val requester = remember { BringIntoViewRequester() }
    val raised = remember { raisedWhileUp }
    LaunchedEffect(Unit) { if (raised) requester.bringIntoView() }
    Box(Modifier.bringIntoViewRequester(requester)) { content() }
}
