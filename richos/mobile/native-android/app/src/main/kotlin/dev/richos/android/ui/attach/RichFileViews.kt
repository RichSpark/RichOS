package dev.richos.android.ui.attach

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.pdf.PdfRenderer
import android.media.MediaPlayer
import android.os.ParcelFileDescriptor
import android.widget.VideoView
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.richos.android.core.FileFetch
import dev.richos.android.core.FileFetchState
import dev.richos.android.core.MediaMoments
import dev.richos.android.core.RichFile
import dev.richos.android.core.RichFileRule
import dev.richos.android.design.Rich
import dev.richos.android.design.RichColors
import dev.richos.android.design.RichIcon
import dev.richos.android.design.RichIcons
import dev.richos.android.design.RoundButton
import dev.richos.android.design.touchTarget
import dev.richos.android.platform.RichFileShare
import dev.richos.android.ui.UiEvent
import dev.richos.android.ui.model.RichFileCard
import dev.richos.android.ui.model.Sizes
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.io.File

/**
 * The files Rich attached, under his words in his bubble (round 36 screen 3, "Phone: in the chat";
 * round 12.1's file style): the page with its type, the name, and one line. Tap one: it comes over
 * from the Mac (the gold ring fills, the line counts the kilobytes, × stops it), then opens in the
 * viewer. A second tap opens it at once. With the Mac out of reach the ring is a clock and the line
 * says "Opens when your Mac is back online".
 */
@Composable
fun RichFileRows(cards: List<RichFileCard>, onEvent: (UiEvent) -> Unit, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth().padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for (card in cards) {
            val moments = card.file.moments
            if (moments == null) RichFileRow(card, onEvent) else RichMediaCard(card, moments, onEvent)
        }
    }
}

@Composable
private fun RichMediaCard(card: RichFileCard, moments: MediaMoments, onEvent: (UiEvent) -> Unit) {
    val file = card.file
    val fetch = card.fetch
    val colors = Rich.colors
    val type = Rich.type
    val uri = LocalUriHandler.current
    val shape = RoundedCornerShape(12.dp)
    val status = when {
        !file.exists -> "The recording was moved, deleted or changed."
        fetch?.state == FileFetchState.COMING -> "Coming over from your Mac"
        fetch?.state == FileFetchState.WAITING -> RichFileRule.WAITING_LINE
        fetch?.state == FileFetchState.FAILED -> fetch.reason ?: RichFileRule.FAILED
        else -> null
    }
    Column(
        Modifier.fillMaxWidth().background(colors.ground, shape).border(1.dp, colors.lineFaint, shape).padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        BasicText(file.name, style = type.body.copy(color = colors.ink))
        val kind = if (file.youtubeId != null) "YouTube" else if (file.mediaType == "video") "Video" else "Audio"
        val location = if (file.youtubeId == null) " · on your Mac" else ""
        BasicText("$kind · ${MediaMoments.time(moments.durationMs)}$location", style = type.read.copy(color = colors.inkSoft))
        if (status != null) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (fetch?.state == FileFetchState.COMING) {
                    Canvas(Modifier.size(22.dp)) {
                        val stroke = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round)
                        drawCircle(colors.lineFaint, style = stroke)
                        drawArc(colors.signal, -90f, 360f * RichFileRule.fraction(fetch).toFloat(), false, style = stroke)
                    }
                }
                BasicText(status, style = type.read.copy(color = colors.inkSoft))
            }
        }
        Canvas(Modifier.fillMaxWidth().height(32.dp).graphicsLayer { alpha = if (file.exists) 1f else 0.45f }) {
            val levels = moments.waveform.orEmpty().mapNotNull { it.digitToIntOrNull() }
            if (levels.isEmpty()) {
                drawLine(colors.inkSoft, Offset(0f, size.height / 2), Offset(size.width, size.height / 2), 2.dp.toPx())
            } else {
                val step = size.width / levels.size
                levels.forEachIndexed { index, level ->
                    val height = maxOf(2f, size.height * level / 10f)
                    drawRect(colors.inkSoft, Offset(index * step, (size.height - height) / 2), Size(maxOf(1f, step - 1), height))
                }
            }
            moments.points.forEach { point ->
                val x = size.width * point.startMs / maxOf(1L, moments.durationMs)
                drawLine(colors.signal, Offset(x, 3.dp.toPx()), Offset(x, size.height - 3.dp.toPx()), 3.dp.toPx(), StrokeCap.Round)
            }
        }
        for (point in moments.points) {
            val time = MediaMoments.time(point.startMs)
            val url = file.youtubeId?.let { MediaMoments.youtube(it, point.startMs) }
            val canOpen = file.exists && (file.youtubeId == null || url != null)
            val tap = if (!canOpen) Modifier else Modifier.clickable(role = Role.Button, onClickLabel = "$time ${point.label}") {
                if (url != null) uri.openUri(url) else onEvent(UiEvent.OpenRichFile(file.id, point.startMs))
            }
            Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).then(tap)
                .semantics { testTag = "media.${file.id}.${point.startMs}" }.padding(vertical = 2.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                BasicText(if (!canOpen) time else if (url == null) "▶ $time" else "↗ $time",
                    Modifier.border(1.dp, if (canOpen) colors.signal else colors.ground, CircleShape).padding(horizontal = 6.dp, vertical = 3.dp),
                    style = type.read.copy(color = colors.ink))
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    BasicText(point.label, style = type.body.copy(color = colors.ink))
                    BasicText("“${point.quote}”", style = type.body.copy(color = colors.inkSoft))
                }
            }
        }
    }
}

@Composable
fun RichMediaDock(card: RichFileCard, onEvent: (UiEvent) -> Unit) {
    val fetch = card.fetch ?: return
    val path = fetch.path ?: return
    val selection = fetch.media ?: return
    val moments = card.file.moments ?: return
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val colors = Rich.colors
    val type = Rich.type
    val player = remember(path) { VideoView(context) }
    var media by remember(path) { mutableStateOf<MediaPlayer?>(null) }
    var playing by remember(path) { mutableStateOf(false) }
    var ended by remember(path) { mutableStateOf(false) }
    var failed by remember(path) { mutableStateOf(false) }
    var position by remember(path) { mutableLongStateOf(selection.startMs) }
    var firstTap by remember(path) { mutableStateOf(true) }
    val foreground = rememberUpdatedState(owner)
    fun toggle() {
        if (player.isPlaying || playing) {
            player.pause()
            position = player.currentPosition.toLong()
            playing = false
        } else if (owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
            ended = false
            player.start()
            playing = true
        }
    }
    DisposableEffect(player, owner) {
        val lifecycle = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_PAUSE) { player.pause(); playing = false }
        }
        owner.lifecycle.addObserver(lifecycle)
        player.setOnPreparedListener { media = it }
        player.setOnCompletionListener {
            playing = false
            ended = true
            position = moments.durationMs
        }
        player.setOnErrorListener { _, _, _ -> failed = true; playing = false; true }
        player.setVideoPath(path)
        onDispose {
            owner.lifecycle.removeObserver(lifecycle)
            player.stopPlayback()
        }
    }
    LaunchedEffect(player, media, selection) {
        val prepared = media ?: return@LaunchedEffect
        if (!foreground.value.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) return@LaunchedEffect
        val active = moments.points.lastOrNull { it.startMs <= position }
        if (!firstTap && !ended && active?.startMs == selection.startMs) {
            toggle()
        } else {
            firstTap = false
            ended = false
            prepared.seekTo(selection.startMs, MediaPlayer.SEEK_CLOSEST)
            position = selection.startMs
            player.start()
            playing = true
        }
    }
    LaunchedEffect(player, playing) {
        while (playing) {
            delay(250)
            if (player.isPlaying) position = player.currentPosition.toLong()
        }
    }
    val shape = RoundedCornerShape(12.dp)
    Column(Modifier.fillMaxWidth().background(colors.surface, shape).border(1.dp, colors.lineFaint, shape).padding(8.dp)) {
        AndroidView(factory = { player }, modifier = if (card.file.mediaType == "video") Modifier.fillMaxWidth().height(110.dp)
            else Modifier.size(1.dp).graphicsLayer { alpha = 0f })
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            BasicText(if (playing) "Ⅱ" else "▶", Modifier.heightIn(min = 44.dp)
                .clickable(enabled = media != null && !failed, role = Role.Button, onClickLabel = if (playing) "Pause recording" else "Play recording") { toggle() }
                .padding(10.dp), style = type.body.copy(color = colors.ink))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                val point = moments.points.lastOrNull { it.startMs <= position }
                val label = if (failed) "This phone could not play the recording." else
                    point?.let { MediaMoments.time(it.startMs) + " · " + it.label } ?: card.file.name
                BasicText(label, style = type.body.copy(color = colors.ink), maxLines = 2)
                Canvas(Modifier.fillMaxWidth().height(3.dp)) {
                    drawLine(colors.lineFaint, Offset.Zero, Offset(size.width, 0f), 3.dp.toPx())
                    val progress = (position.toFloat() / maxOf(1L, moments.durationMs)).coerceIn(0f, 1f)
                    drawLine(colors.ink, Offset.Zero, Offset(size.width * progress, 0f), 3.dp.toPx())
                }
                BasicText(MediaMoments.time(position) + " / " + MediaMoments.time(moments.durationMs), style = type.read.copy(color = colors.inkSoft))
            }
            BasicText("×", Modifier.heightIn(min = 44.dp).clickable(role = Role.Button, onClickLabel = "Close recording") {
                onEvent(UiEvent.CloseRichFile(card.file.id))
            }.padding(10.dp), style = type.body.copy(color = colors.ink))
        }
    }
}

@Composable
private fun RichFileRow(card: RichFileCard, onEvent: (UiEvent) -> Unit) {
    val c = Rich.colors
    val t = Rich.type
    val file = card.file
    val fetch = card.fetch
    val state = fetch?.state
    val line = when {
        state == FileFetchState.COMING -> RichFileRule.progress(fetch!!.received, fetch.total ?: file.bytes)
        state == FileFetchState.WAITING -> RichFileRule.WAITING_LINE
        state == FileFetchState.FAILED -> fetch!!.reason ?: RichFileRule.FAILED
        !file.exists -> RichFileRule.GONE
        else -> file.meta
    }
    val shape = RoundedCornerShape(14.dp)
    Row(
        Modifier.fillMaxWidth()
            .clip(shape)
            .background(c.ground, shape)
            .border(1.dp, c.lineFaint, shape)
            .clickable(role = Role.Button, onClickLabel = "Open") {
                onEvent(if (state == FileFetchState.COMING) UiEvent.StopRichFile(file.id) else UiEvent.OpenRichFile(file.id))
            }
            .semantics { contentDescription = "${file.name}, $line"; testTag = "rich-file" }
            .padding(start = 6.dp, end = 12.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(Modifier.size(52.dp), contentAlignment = Alignment.Center) {
            FileIcon(file.ext)
            when (state) {
                FileFetchState.COMING -> FetchDisc(RichIcons.Close, "Stop", RichFileRule.fraction(fetch!!)) { onEvent(UiEvent.StopRichFile(file.id)) }
                FileFetchState.WAITING -> FetchDisc(RichIcons.Clock, "Waiting for your Mac", null, null)
                else -> Unit
            }
        }
        Column(Modifier.weight(1f)) {
            BasicText(Sizes.breakable(file.name), style = t.bodyStrong.copy(color = c.ink, lineHeight = t.body.lineHeight))
            BasicText(line, style = t.read.copy(color = c.inkSoft, fontFeatureSettings = "tnum"), modifier = Modifier.padding(top = 2.dp))
        }
    }
}

/** The disc over the page while a file comes over (the gold ring and ×) or waits (the clock). */
@Composable
private fun FetchDisc(icon: androidx.compose.ui.graphics.vector.ImageVector, spoken: String, progress: Float?, onTap: (() -> Unit)?) {
    val c = Rich.colors
    val arc = c.signal
    val track = c.lineFaint
    Box(
        Modifier
            .then(if (onTap != null) Modifier.clickable(role = Role.Button, onClick = onTap).touchTarget() else Modifier)
            .semantics { contentDescription = spoken; if (onTap != null) role = Role.Button }
            .size(40.dp)
            .background(c.surface, CircleShape)
            .border(1.dp, c.lineFaint, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        if (progress != null) {
            Canvas(Modifier.size(34.dp)) {
                val w = 3.dp.toPx()
                val inset = w / 2
                val arcSize = Size(size.width - w, size.height - w)
                drawArc(track, 0f, 360f, false, Offset(inset, inset), arcSize, style = Stroke(w))
                drawArc(arc, -90f, 360f * progress, false, Offset(inset, inset), arcSize, style = Stroke(w, cap = StrokeCap.Round))
            }
        }
        RichIcon(icon, c.ink, 16.dp)
    }
}

/**
 * The phone's viewer for a file Rich attached (round 36 "Phone: opened"): black, Close and who sent
 * it at the top, Share on the right (the phone's share sheet: Save to Files, Mail, Messages, …), the
 * file in the middle, its name below. The app draws two kinds itself, from the kept file: a PDF's
 * first page as Android draws it, and a picture. Every other kind (a spreadsheet, a Word document,
 * a slide deck) is handed on the tap to the app on this phone that opens it (the system's open-with,
 * ACTION_VIEW with the file's type); with none, the viewer says so in one line beside Share.
 * Nothing is ever drawn in a file's place.
 */
@Composable
fun RichFileViewer(file: RichFile, fetch: FileFetch, time: String, onEvent: (UiEvent) -> Unit) {
    val context = LocalContext.current
    val path = fetch.path ?: return
    val close = { onEvent(UiEvent.CloseRichFile(file.id)) }
    if (file.ext.uppercase() !in PAGES) {
        // Handed to the phone's own app on the tap; the viewer shows only when no app here opens it.
        var noApp by remember(path) { mutableStateOf(false) }
        LaunchedEffect(path) { if (RichFileShare.view(context, path, file.name)) close() else noApp = true }
        if (!noApp) return
        ViewerFrame(file, path, time, close) { Unshown("No app on this phone opens ${file.ext.uppercase()} files. Share sends it to one that can.") }
        return
    }
    val shown by produceState<Shown?>(null, path) { value = withContext(Dispatchers.IO) { firstPage(path, file.ext)?.let { Shown.Page(it.first, it.second) } ?: Shown.Unreadable } }
    val pages = (shown as? Shown.Page)?.pages?.takeIf { it > 0 }
    ViewerFrame(file, path, time, close, pages) {
        when (val s = shown) {
            null -> Unit // a moment on the black while the file is read
            is Shown.Page -> BoxWithConstraints(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                val aspect = s.image.width.toFloat() / s.image.height
                Image(
                    s.image, contentDescription = file.name, contentScale = ContentScale.Fit,
                    modifier = Modifier.fillMaxWidth().aspectRatio(aspect, matchHeightConstraintsFirst = maxWidth / maxHeight > aspect)
                        .clip(RoundedCornerShape(4.dp)).background(androidx.compose.ui.graphics.Color.White),
                )
            }
            Shown.Unreadable -> Unshown("This file could not be shown here. Share sends it to an app that can.")
        }
    }
}

/** The kinds the viewer draws itself, from the file: PDFs and pictures. */
private val PAGES = setOf("PDF", "PNG", "JPG", "JPEG", "GIF", "WEBP", "HEIC")

/** What the viewer shows, read from the kept file. */
private sealed interface Shown {
    class Page(val image: ImageBitmap, val pages: Int) : Shown
    /** A PDF or picture this phone could not draw. */
    data object Unreadable : Shown
}

/** Said in the file's place when it is not shown here: one plain line, with the file's glyph. */
@Composable
private fun Unshown(sentence: String) {
    val f = RichColors.Fixed
    val t = Rich.type
    Column(Modifier.fillMaxSize().padding(horizontal = 24.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        BasicText(sentence, style = t.body.copy(color = f.viewerInk, textAlign = TextAlign.Center))
    }
}

/** The viewer's black frame: Close and who sent it, Share, [content] between, the name (and its pages) below. */
@Composable
private fun ViewerFrame(file: RichFile, path: String, time: String, close: () -> Unit, pages: Int? = null, content: @Composable () -> Unit) {
    val f = RichColors.Fixed
    val t = Rich.type
    val context = LocalContext.current
    val appear = remember { Animatable(0f) }
    LaunchedEffect(Unit) { appear.animateTo(1f, tween(240)) }
    Box(
        Modifier.fillMaxSize()
            .graphicsLayer { alpha = appear.value }
            .background(f.viewerGround)
            .semantics { paneTitle = "File"; testTag = "rich-file-viewer" },
    ) {
        Box(
            Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).padding(top = 64.dp, bottom = 72.dp, start = 12.dp, end = 12.dp),
            contentAlignment = Alignment.Center,
        ) { content() }
        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(start = 14.dp, end = 14.dp, top = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            RoundButton(RichIcons.Close, "Close", close, onScene = true)
            Column(Modifier.weight(1f)) {
                BasicText("Rich", style = t.bodyStrong.copy(color = f.viewerInk))
                BasicText(if (time == "Now") "Today, just now" else "Today, $time", style = t.read.copy(color = f.viewerInkSoft))
            }
            RoundButton(RichIcons.Share, "Share", { RichFileShare.share(context, path, file.name) }, onScene = true)
        }
        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing)
                .padding(start = 20.dp, end = 20.dp, bottom = 14.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            BasicText(Sizes.breakable(file.name) + (pages?.let { " · page 1 of $it" } ?: ""),
                style = t.read.copy(color = f.viewerInk, textAlign = TextAlign.Center))
        }
    }
}

/** A PDF's first page as Android draws it (and its page count), or a picture; null for anything else. */
private fun firstPage(path: String, ext: String): Pair<ImageBitmap, Int>? = runCatching {
    val file = File(path)
    when (ext.uppercase()) {
        "PDF" -> ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { fd ->
            PdfRenderer(fd).use { pdf ->
                pdf.openPage(0).use { p ->
                    val scale = 1600f / maxOf(p.width, p.height)
                    val bmp = Bitmap.createBitmap((p.width * scale).toInt(), (p.height * scale).toInt(), Bitmap.Config.ARGB_8888)
                    bmp.eraseColor(android.graphics.Color.WHITE)
                    p.render(bmp, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                    bmp.asImageBitmap() to pdf.pageCount
                }
            }
        }
        "PNG", "JPG", "JPEG", "GIF", "WEBP", "HEIC" -> {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(path, bounds)
            var sample = 1
            while (maxOf(bounds.outWidth, bounds.outHeight) / sample > 2400) sample *= 2
            BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample })?.asImageBitmap()?.let { it to 0 }
        }
        else -> null
    }
}.getOrNull()
