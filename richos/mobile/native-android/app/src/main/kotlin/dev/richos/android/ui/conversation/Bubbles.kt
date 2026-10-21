package dev.richos.android.ui.conversation

import dev.richos.android.design.rememberBoundedRotation
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.StartOffset
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import dev.richos.android.design.Rich
import dev.richos.android.design.RichColors
import dev.richos.android.design.RichIcon
import dev.richos.android.design.RichIcons
import dev.richos.android.design.RichMotion
import dev.richos.android.design.Spinner
import dev.richos.android.design.floating
import dev.richos.android.design.pressScale
import dev.richos.android.design.signalUnderline
import dev.richos.android.design.touchTarget
import dev.richos.android.ui.UiEvent
import dev.richos.android.ui.attach.AlbumContent
import dev.richos.android.ui.attach.FileContent
import dev.richos.android.ui.attach.ReferenceChip
import dev.richos.android.ui.attach.RichFileRows
import dev.richos.android.ui.model.UploadStatus
import dev.richos.android.ui.model.Body
import dev.richos.android.ui.model.Delivery
import dev.richos.android.ui.model.Message
import dev.richos.android.ui.model.ReplyAudio
import dev.richos.android.ui.model.Speaker
import dev.richos.android.ui.model.TimeLabels
import dev.richos.android.ui.model.Waves

/**
 * One row of the conversation (round 12 `.row`): Rich in the cool surface on the left, you in the
 * warm gold wash on the right. The last bubble of a run squares its outer bottom corner.
 */
@Composable
fun MessageRow(message: Message, tail: Boolean, onEvent: (UiEvent) -> Unit, modifier: Modifier = Modifier) {
    val mine = message.speaker == Speaker.ME
    val body = message.body
    val full = body is Body.Question || body is Body.Voice || body is Body.Album || body is Body.File || message.files.isNotEmpty()
    val fraction = when (body) {
        is Body.Question -> 0.92f
        is Body.Voice, is Body.Album -> 0.74f
        else -> if (mine) 0.78f else 0.84f
    }
    // Albums stop at 290 dp and files at 300 dp (attachments NOTES), whatever the phone's width.
    val cap = when (body) {
        is Body.Album -> 290.dp
        is Body.File -> 300.dp
        else -> androidx.compose.ui.unit.Dp.Unspecified
    }
    Box(modifier.fillMaxWidth(), contentAlignment = if (mine) Alignment.CenterEnd else Alignment.CenterStart) {
        Column(
            Modifier.fillMaxWidth(fraction).then(if (cap != androidx.compose.ui.unit.Dp.Unspecified) Modifier.widthIn(max = cap) else Modifier),
            horizontalAlignment = if (mine) Alignment.End else Alignment.Start,
        ) {
            Bubble(message, tail, onEvent, Modifier.then(if (full) Modifier.fillMaxWidth() else Modifier))
            if (message.delivery == Delivery.ATTENTION) AttentionLine(message, onEvent)
        }
    }
}

@Composable
private fun Bubble(message: Message, tail: Boolean, onEvent: (UiEvent) -> Unit, modifier: Modifier) {
    val c = Rich.colors
    val mine = message.speaker == Speaker.ME
    val r = 20.dp
    val squared = 6.dp
    val shape = RoundedCornerShape(
        topStart = r, topEnd = r,
        bottomStart = if (!mine && tail) squared else r,
        bottomEnd = if (mine && tail) squared else r,
    )
    val fill = if (mine) c.mine else c.surface
    val glow = if (message.focused) focusGlow() else 0f
    var m = modifier.floating(shape).background(fill, shape)
    if (mine) m = m.border(1.dp, c.signalWash, shape)
    if (message.focused) {
        val signal = c.signal
        m = m.drawBehind {
            // The resting 2 dp gold ring, plus the glow that rises to 3 dp and fades once (2.2 s).
            val w = (2.dp.toPx()) + glow * 3.dp.toPx()
            val inset = -w / 2
            drawRoundRect(
                signal,
                topLeft = Offset(inset, inset),
                size = Size(size.width - 2 * inset, size.height - 2 * inset),
                cornerRadius = CornerRadius(r.toPx() + w / 2),
                style = Stroke(w),
            )
        }
    }
    when (val body = message.body) {
        is Body.Question -> QuestionCard(body, onEvent, modifier)
        is Body.Text -> TextBubble(message, body, Modifier.then(m).padding(start = 14.dp, end = 14.dp, top = 10.dp, bottom = 8.dp), onEvent)
        is Body.Voice -> VoiceBubble(message, body, Modifier.then(m).padding(start = 8.dp, end = 12.dp, top = 8.dp, bottom = 8.dp), onEvent)
        is Body.Album -> Box(Modifier.then(m)) {
            AlbumContent(
                body.photos, body.caption, message.upload, message.progress,
                meta = { onPhoto -> Meta(message, onPhoto = onPhoto) },
                onOpen = { i -> onEvent(UiEvent.OpenViewer(message.id, i)) },
                onDisc = { onEvent(if (message.upload == UploadStatus.ATTENTION) UiEvent.UploadRetry(message.id) else UiEvent.UploadStop(message.id)) },
            )
        }
        is Body.File -> Box(Modifier.then(m)) {
            FileContent(
                body.file, body.caption, message.upload, message.progress,
                meta = { Meta(message) },
                onOpen = { onEvent(UiEvent.OpenViewer(message.id, 0)) },
                onDisc = { onEvent(if (message.upload == UploadStatus.ATTENTION) UiEvent.UploadRetry(message.id) else UiEvent.UploadStop(message.id)) },
            )
        }
    }
}

@Composable
private fun focusGlow(): Float {
    val a = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        a.animateTo(1f, tween((RichMotion.FOCUS_GLOW_MS * 0.3f).toInt(), easing = RichMotion.OutQuint))
        a.animateTo(0f, tween((RichMotion.FOCUS_GLOW_MS * 0.7f).toInt(), easing = RichMotion.OutQuint))
    }
    return a.value
}

@Composable
private fun TextBubble(message: Message, body: Body.Text, modifier: Modifier, onEvent: (UiEvent) -> Unit) {
    val c = Rich.colors
    val t = Rich.type
    val mine = message.speaker == Speaker.ME
    val style = (if (mine) t.body else t.answer).copy(color = c.ink)
    Column(modifier) {
        message.ref?.let { ReferenceChip(it, onEvent) }
        when {
            message.replying -> Row(
                Modifier.height(with(androidx.compose.ui.platform.LocalDensity.current) { (t.answer.fontSize.toPx() * 1.5f).toDp() })
                    .semantics { contentDescription = "Rich is replying" },
                verticalAlignment = Alignment.CenterVertically,
            ) { ThinkingDots() }
            message.streaming -> StreamingText(body.text, style)
            // Still, and plainly unfinished: the ellipsis stands where the caret was. No time, which
            // belongs to a finished message; the Mac's row replaces this once the stream is back.
            message.unfinished -> BasicText(body.text.trimEnd() + "…", style = style)
            body.text.isNotEmpty() || message.files.isEmpty() -> BasicText(body.text, style = style)
        }
        // The files Rich attached, under his words (round 36 screen 3).
        if (message.files.isNotEmpty()) RichFileRows(message.files, onEvent)
        if (!message.replying && !message.streaming && !message.unfinished) Meta(message, Modifier.align(Alignment.End).padding(top = 4.dp))
        if (!mine && message.audio != ReplyAudio.NONE) ReplyAudioRow(message, onEvent)
    }
}

/** Words land as they come, with a gold caret at the end that blinks once a second. */
@Composable
private fun StreamingText(text: String, style: androidx.compose.ui.text.TextStyle) {
    val signal = Rich.colors.signal
    val blink by rememberInfiniteTransition(label = "caret").animateFloat(
        1f, 0f,
        infiniteRepeatable(keyframes { durationMillis = 1000; 1f at 0; 1f at 499; 0f at 500; 0f at 999 }),
        label = "caret",
    )
    val content = remember(signal) {
        mapOf(
            "caret" to InlineTextContent(Placeholder(6.sp, 1.1.em, PlaceholderVerticalAlign.TextCenter)) {
                Box(Modifier.padding(start = 2.dp).width(2.dp).height(20.dp).graphicsLayer { alpha = blink }.background(signal))
            },
        )
    }
    BasicText(
        buildAnnotatedString { append(text); append(" "); appendInlineContent("caret", "…") },
        style = style,
        inlineContent = content,
    )
}

/** Three gold dots breathing (1.2 s, staggered 0.15 s): Rich is replying. No sentence. */
@Composable
fun ThinkingDots(modifier: Modifier = Modifier) {
    val signal = Rich.colors.signal
    val transition = rememberInfiniteTransition(label = "think")
    Row(modifier.padding(start = 2.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        repeat(3) { i ->
            val p by transition.animateFloat(
                0f, 1f,
                infiniteRepeatable(tween(1200, easing = LinearEasing), RepeatMode.Restart, StartOffset(i * 150)),
                label = "dot$i",
            )
            // 0 → 50% → 100%: up 3 dp and full opacity at the middle, 45% at rest.
            val k = 1f - kotlin.math.abs(2f * p - 1f)
            val eased = RichMotion.InOut.transform(k)
            Box(
                Modifier.size(7.dp)
                    .graphicsLayer { translationY = -3.dp.toPx() * eased; alpha = 0.45f + 0.55f * eased }
                    .background(signal, CircleShape),
            )
        }
    }
}

/** Time inside the bubble, and for your messages one honest delivery state in place of receipts. */
@Composable
private fun Meta(message: Message, modifier: Modifier = Modifier, onPhoto: Boolean = false) {
    val c = Rich.colors
    val t = Rich.type
    // On a photo the meta sits in a dark chip, the same in both themes (attachments GAP A5).
    val soft = if (onPhoto) RichColors.Fixed.photoDiscInk else c.inkSoft
    val mine = message.speaker == Speaker.ME
    val label = when (message.delivery) {
        Delivery.SENDING -> "Sending…"
        Delivery.WAITING -> "Waiting to send"
        else -> null
    }
    val spoken = buildString {
        if (label != null) append(label).append(", ")
        append(message.time)
        if (mine && message.delivery == Delivery.SENT) append(", sent")
        if (mine && message.delivery == Delivery.ATTENTION) append(", not sent")
        message.via?.let { append(", shared from ").append(it) }
    }
    FlowRow(
        modifier.clearAndSetSemantics { contentDescription = spoken },
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.End),
        itemVerticalAlignment = Alignment.CenterVertically,
    ) {
        message.via?.let { BasicText("Shared from $it", style = t.read.copy(color = soft)) }
        // DECLARED SKIPPABLE, confirmed by the CEO 2026-09-24 ("14px, as the mockup", ceo-decisions
        // §83; "The type scale": a node in the 14 px tier is a claim that it is skippable, because the
        // status glyph beside it carries the same meaning): the delivery words inside a bubble are set
        // as round 12.1 sets them, in `.meta` with the time, 14 sp medium, which lets a voice
        // message's status share the duration's line (Urban's 2026-09-24 audit G14; iOS `Meta` does
        // the same). The words mirror what is also shown in another form: the delivery glyph beside
        // them, the row's TalkBack label, and, while core holds messages back, the "Waiting to send"
        // card above the composer at 16 sp. Contrast stays at the text floor: ink-soft on your
        // bubble 6.90:1 dark, 6.24:1 light (contrast.py on the rendered conv-pending frames).
        if (label != null) BasicText(label, style = t.stamp.copy(color = soft))
        // DECLARED SKIPPABLE (round-12 NOTES "Type"): the timestamp inside a bubble, 14 sp.
        BasicText(message.time, style = t.stamp.copy(color = soft))
        if (mine) DeliveryGlyph(message.delivery, if (onPhoto) soft else null)
    }
}

@Composable
private fun DeliveryGlyph(delivery: Delivery, tint: androidx.compose.ui.graphics.Color? = null) {
    val c = Rich.colors
    when (delivery) {
        Delivery.SENT -> RichIcon(RichIcons.Check, tint ?: c.inkSoft, 15.dp)
        Delivery.WAITING -> RichIcon(RichIcons.Clock, tint ?: c.inkSoft, 15.dp)
        Delivery.ATTENTION -> RichIcon(RichIcons.Alert, tint ?: c.danger, 15.dp)
        Delivery.SENDING -> {
            val turn by rememberBoundedRotation(periodMillis = 1_100)
            RichIcon(RichIcons.Spinner, tint ?: c.inkSoft, 15.dp, Modifier.graphicsLayer { rotationZ = turn })
        }
    }
}

/** "Not sent · needs attention  Discard", under the bubble. */
@Composable
private fun AttentionLine(message: Message, onEvent: (UiEvent) -> Unit) {
    val c = Rich.colors
    val t = Rich.type
    Column(Modifier.padding(top = 6.dp), horizontalAlignment = Alignment.End) {
    FlowRow(
        Modifier,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
        verticalArrangement = Arrangement.Center,
    ) {
        BasicText(
            buildAnnotatedString {
                withStyle(SpanStyle(color = c.danger, fontWeight = FontWeight.SemiBold)) { append("Not sent") }
                append(" · needs attention")
            },
            style = t.read.copy(color = c.ink),
            modifier = Modifier.align(Alignment.CenterVertically),
        )
        val id = message.outboxClientId ?: message.id
        if (message.upload != null) {
            // An attachment picks up where it stopped (attachments NOTES "Try again resumes").
            BasicText(
                "Try again",
                style = t.read.copy(color = c.ink, fontWeight = FontWeight.Medium),
                modifier = Modifier
                    .clickable(role = Role.Button) { onEvent(UiEvent.UploadRetry(message.id)) }
                    .touchTarget()
                    .semantics { contentDescription = "Try sending again" }
                    .signalUnderline(),
            )
        }
        BasicText(
            "Discard",
            style = t.read.copy(color = c.ink, fontWeight = FontWeight.Medium),
            modifier = Modifier
                .clickable(role = Role.Button) { onEvent(UiEvent.Discard(id)) }
                .touchTarget()
                .semantics { contentDescription = "Discard this unsent message" }
                .signalUnderline(),
        )
    }
    // Why the Mac refused it, in its own words (iPhone parity `7faec1fda`); nothing for a bare code.
    message.refusal?.let { reason ->
        BasicText(
            reason,
            style = t.read.copy(color = c.ink, textAlign = TextAlign.End),
            modifier = Modifier.padding(top = 4.dp).testTag("reason:${message.id}"),
        )
    }
    }
}

/** Your voice message: the gold play button, the recording's own waveform, its length. */
@Composable
private fun VoiceBubble(message: Message, body: Body.Voice, modifier: Modifier, onEvent: (UiEvent) -> Unit) {
    val c = Rich.colors
    val t = Rich.type
    val playing = message.playProgress > 0f
    Column(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            PlayButton(
                playing = playing,
                label = if (playing) "Pause voice message" else if (body.durationMs > 0) "Play voice message, ${TimeLabels.duration(body.durationMs)}" else "Play voice message",
                onClick = { onEvent(UiEvent.PlayVoice(message.id)) },
            )
            Waveform(body.wave, message.playProgress, Modifier.weight(1f).height(34.dp), barMax = 26f)
        }
        FlowRow(
            Modifier.fillMaxWidth().padding(start = 54.dp, top = 4.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            val shown = if (playing) (body.durationMs * message.playProgress).toLong() else body.durationMs
            // A voice row with no length (the wire carries none) shows its time and state only.
            if (body.durationMs > 0) BasicText(
                TimeLabels.duration(shown),
                style = t.read.copy(color = if (playing && c.isDark) c.signal else c.ink, fontWeight = FontWeight.Medium, fontFeatureSettings = "tnum"),
                modifier = Modifier.align(Alignment.CenterVertically).padding(end = 10.dp),
            )
            Meta(message, Modifier.align(Alignment.CenterVertically))
        }
    }
}

/** `.play`: the 44 dp gold circle with a play or stop glyph (7.68:1 dark, 4.72:1 light). */
@Composable
fun PlayButton(playing: Boolean, label: String, onClick: () -> Unit, size: androidx.compose.ui.unit.Dp = 44.dp) {
    val c = Rich.colors
    val source = remember { MutableInteractionSource() }
    Box(
        Modifier.clickable(source, indication = null, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label }
            .touchTarget()
            .pressScale(source)
            .size(size)
            .floating(CircleShape, 8.dp)
            .background(c.signal, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        RichIcon(if (playing) RichIcons.Stop else RichIcons.Play, c.onSignal, 20.dp)
    }
}

/**
 * The waveform (`.wave`): bars of equal share, 2 dp apart, 2–4 dp wide, left aligned; a level
 * under 0.12 is a quiet dot. Bars already played are full ink, the rest 32%.
 */
@Composable
fun Waveform(levels: List<Float>, progress: Float, modifier: Modifier, barMax: Float = 26f, barMin: Float = 4f) {
    val ink = Rich.colors.ink
    Canvas(modifier.clearAndSetSemantics { }) {
        if (levels.isEmpty()) return@Canvas
        val gap = 2.dp.toPx()
        val n = levels.size
        val bw = ((size.width - gap * (n - 1)) / n).coerceIn(2.dp.toPx(), 4.dp.toPx())
        val cy = size.height / 2
        levels.forEachIndexed { i, v ->
            val x = i * (bw + gap)
            val quiet = v < 0.12f
            val h = if (quiet) 3.dp.toPx() else (barMin + v * barMax).dp.toPx()
            val on = i.toFloat() / n < progress
            val alpha = when {
                quiet -> 0.22f
                on -> 1f
                else -> 0.32f
            }
            drawRoundRect(
                ink.copy(alpha = alpha),
                topLeft = Offset(x, cy - h / 2),
                size = Size(bw, h),
                cornerRadius = CornerRadius(2.dp.toPx()),
            )
        }
    }
}

/** Rich's bubble grows a Hear it control; preparing is quiet; playing shows a live waveform and Stop. */
@Composable
private fun ReplyAudioRow(message: Message, onEvent: (UiEvent) -> Unit) {
    val c = Rich.colors
    val t = Rich.type
    val lineFaint = c.lineFaint
    FlowRow(
        Modifier.padding(top = 10.dp).fillMaxWidth()
            .drawBehind { drawLine(lineFaint, Offset(0f, 0f), Offset(size.width, 0f), 1.dp.toPx()) }
            .padding(top = 10.dp),
        itemVerticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        when (message.audio) {
            ReplyAudio.PLAYING -> {
                MiniButton("Stop", RichIcons.Stop, filled = true, spoken = "Stop the reply") { onEvent(UiEvent.StopReply(message.id)) }
                Waveform(Waves.forSeed(5, 28), 12f / 28f, Modifier.weight(1f).widthIn(min = 120.dp).height(34.dp), barMax = 20f)
            }
            ReplyAudio.PREPARING -> {
                MiniButton("Hear it", RichIcons.Spinner, filled = false, spoken = "Hear it, preparing") { }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Spinner(16.dp)
                    Spacer(Modifier.width(6.dp))
                    BasicText("Preparing the audio…", style = t.read.copy(color = c.inkSoft))
                }
            }
            else -> MiniButton("Hear it", RichIcons.Play, filled = true, spoken = "Hear this reply") { onEvent(UiEvent.HearReply(message.id)) }
        }
    }
}

@Composable
private fun MiniButton(label: String, icon: androidx.compose.ui.graphics.vector.ImageVector, filled: Boolean, spoken: String, onClick: () -> Unit) {
    val c = Rich.colors
    val t = Rich.type
    val shape = RoundedCornerShape(20.dp)
    val source = remember { MutableInteractionSource() }
    val fg = if (filled) c.onSignal else c.ink
    Row(
        Modifier.clickable(source, indication = null, role = Role.Button, onClick = onClick)
            .clearAndSetSemantics { contentDescription = spoken }
            .touchTarget()
            .pressScale(source)
            .heightIn(min = 40.dp)
            .then(if (filled) Modifier.background(c.signal, shape) else Modifier.border(1.dp, c.line, shape))
            .padding(start = 10.dp, end = 14.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        RichIcon(icon, fg, 18.dp)
        BasicText(label, style = t.readStrong.copy(color = fg))
    }
}
