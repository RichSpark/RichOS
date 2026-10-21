package dev.richos.android.ui.attach

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.richos.android.core.FileFetch
import dev.richos.android.core.FileFetchState
import dev.richos.android.core.RichFile
import dev.richos.android.core.RichFileRule
import dev.richos.android.core.SheetReader
import dev.richos.android.core.SheetTable
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
        for (card in cards) RichFileRow(card, onEvent)
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
 * file in the middle, its name below. Everything shown is read from the kept file itself: a PDF's
 * first page as Android draws it, a picture, a spreadsheet's first sheet ([SheetReader]). Any other
 * kind (a Word document, a slide deck) opens straight in the app on this phone that opens it; with
 * none, the viewer says so beside Share. Nothing is ever drawn in a file's place.
 */
@Composable
fun RichFileViewer(file: RichFile, fetch: FileFetch, time: String, onEvent: (UiEvent) -> Unit) {
    val context = LocalContext.current
    val path = fetch.path ?: return
    val close = { onEvent(UiEvent.CloseRichFile(file.id)) }
    if (!showsItself(file.ext)) {
        // Handed to the phone's own app on the tap; the viewer shows only when no app here opens it.
        var noApp by remember(path) { mutableStateOf(false) }
        LaunchedEffect(path) { if (RichFileShare.view(context, path, file.name)) close() else noApp = true }
        if (!noApp) return
        ViewerFrame(file, path, time, close, foot = Sizes.breakable(file.name)) { Unshown(file.ext, noApp = true) }
        return
    }
    val shown by produceState<Shown?>(null, path) { value = withContext(Dispatchers.IO) { read(path, file.ext) } }
    val pages = (shown as? Shown.Page)?.pages?.takeIf { it > 0 }
    val cut = (shown as? Shown.Sheet)?.table?.cut == true
    ViewerFrame(
        file, path, time, close,
        foot = Sizes.breakable(file.name) + (pages?.let { " · page 1 of $it" } ?: ""),
        note = if (cut) "Showing the first ${SheetReader.MAX_ROWS} rows. Share sends the whole file." else null,
    ) {
        when (val s = shown) {
            null -> Unit // a moment on the black while the file is read
            is Shown.Page -> BoxWithConstraints(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                val aspect = s.image.width.toFloat() / s.image.height
                Image(
                    s.image, contentDescription = file.name, contentScale = ContentScale.Fit,
                    modifier = Modifier.fillMaxWidth().aspectRatio(aspect, matchHeightConstraintsFirst = maxWidth / maxHeight > aspect)
                        .clip(RoundedCornerShape(4.dp)).background(RichColors.Fixed.sheetGround),
                )
            }
            is Shown.Sheet -> SheetView(s.table, file.name)
            Shown.Unreadable -> Unshown(file.ext, noApp = false)
        }
    }
}

/** The kinds the viewer shows itself, read from the file: PDFs, pictures, spreadsheets. */
private fun showsItself(ext: String) = ext.uppercase() in PAGES || SheetReader.reads(ext)

private val PAGES = setOf("PDF", "PNG", "JPG", "JPEG", "GIF", "WEBP", "HEIC")

/** What the viewer shows, read from the kept file. */
private sealed interface Shown {
    class Page(val image: ImageBitmap, val pages: Int) : Shown
    class Sheet(val table: SheetTable) : Shown
    /** A kind the viewer shows, but this file could not be read as one. */
    data object Unreadable : Shown
}

private fun read(path: String, ext: String): Shown =
    if (SheetReader.reads(ext)) SheetReader.read(path, ext)?.let { Shown.Sheet(it) } ?: Shown.Unreadable
    else firstPage(path, ext)?.let { Shown.Page(it.first, it.second) } ?: Shown.Unreadable

/** Said in the file's place when it is not shown here: no app on this phone opens it, or it could not be read. */
@Composable
private fun Unshown(ext: String, noApp: Boolean) {
    val f = RichColors.Fixed
    val t = Rich.type
    Column(Modifier.fillMaxSize().padding(horizontal = 24.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        FileIcon(ext)
        BasicText(
            if (noApp) "No app on this phone opens ${ext.uppercase()} files." else "This file could not be shown here.",
            style = t.body.copy(color = f.viewerInk, textAlign = TextAlign.Center), modifier = Modifier.padding(top = 16.dp),
        )
        BasicText(
            "Share sends it to an app that can, or open it on your Mac.",
            style = t.read.copy(color = f.viewerInkSoft, textAlign = TextAlign.Center), modifier = Modifier.padding(top = 6.dp),
        )
    }
}

/**
 * A spreadsheet's first sheet as a white sheet in the viewer (round 36 "Phone: opened", the comps):
 * the sheet's name, the first row as its header, numbers to the right. Scrolls both ways; rows are
 * laid out only as they come on screen.
 */
@Composable
private fun SheetView(table: SheetTable, name: String) {
    val f = RichColors.Fixed
    val t = Rich.type
    val cell = t.read.copy(color = f.sheetInk, fontFeatureSettings = "tnum")
    val widths = remember(table) {
        val columns = table.rows.firstOrNull()?.size ?: 0
        List(columns) { c -> (table.rows.maxOfOrNull { it[c].length } ?: 0).coerceIn(3, 28) * 9 + 20 }
    }
    val line = 1.dp
    Box(
        Modifier.fillMaxSize().clip(RoundedCornerShape(4.dp)).background(f.sheetGround)
            .semantics { contentDescription = name }
            .horizontalScroll(rememberScrollState()),
    ) {
        LazyColumn(Modifier.width((widths.sum() + 24).dp).fillMaxHeight(), contentPadding = PaddingValues(12.dp)) {
            table.title?.let { title ->
                item { BasicText(title, style = t.bodyStrong.copy(color = f.sheetInk), modifier = Modifier.padding(bottom = 10.dp)) }
            }
            if (table.rows.isEmpty()) item { BasicText("This sheet is empty.", style = cell) }
            itemsIndexed(table.rows) { r, row ->
                Row(
                    Modifier.then(if (r == 0) Modifier.background(f.sheetHead) else Modifier).drawBehind {
                        val w = line.toPx()
                        if (r == 0) drawRect(f.sheetLine, Offset.Zero, Size(size.width, w))
                        drawRect(f.sheetLine, Offset.Zero, Size(w, size.height))
                    },
                ) {
                    row.forEachIndexed { c, text ->
                        val numeric = text.isNotEmpty() && text.last().let { it.isDigit() || it == '%' } && text.first().let { it.isDigit() || it in "-$€£¥" }
                        Box(
                            Modifier.width(widths[c].dp).drawBehind {
                                val w = line.toPx()
                                drawRect(f.sheetLine, Offset(size.width - w, 0f), Size(w, size.height))
                                drawRect(f.sheetLine, Offset(0f, size.height - w), Size(size.width, w))
                            }.padding(horizontal = 8.dp, vertical = 7.dp),
                        ) {
                            BasicText(
                                text,
                                style = cell.copy(textAlign = if (numeric) TextAlign.End else TextAlign.Start, fontWeight = if (r == 0) FontWeight.SemiBold else null),
                                maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.fillMaxWidth(),
                            )
                        }
                    }
                }
            }
        }
    }
}

/** The viewer's black frame: Close and who sent it, Share, [content] between, [foot] (and a [note]) below. */
@Composable
private fun ViewerFrame(file: RichFile, path: String, time: String, close: () -> Unit, foot: String, note: String? = null, content: @Composable () -> Unit) {
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
            Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)
                .padding(top = 64.dp, bottom = if (note != null) 100.dp else 72.dp, start = 12.dp, end = 12.dp),
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
            BasicText(foot, style = t.read.copy(color = f.viewerInk, textAlign = TextAlign.Center))
            note?.let { BasicText(it, style = t.read.copy(color = f.viewerInkSoft, textAlign = TextAlign.Center), modifier = Modifier.padding(top = 4.dp)) }
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
