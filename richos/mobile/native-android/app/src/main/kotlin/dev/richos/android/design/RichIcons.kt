package dev.richos.android.design

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp

/**
 * The product's own icon convention (round 12 `shared/app.js` `I`): a 24-unit box, no fill,
 * stroked in the current color at width 2 with round caps and joins. Drawn as vectors, never
 * font glyphs, so an icon keeps its size when the text grows (iOS audit F2/F4/F5: glyphs that
 * scaled with text overflowed their buttons; the gear rendered as a color emoji).
 *
 * Every icon is tinted by the composable that draws it; the path color here is a placeholder.
 */
object RichIcons {
    private val Ink = SolidColor(Color.Black)

    private fun rect(x: Float, y: Float, w: Float, h: Float, r: Float): String =
        "M${x + r},${y}h${w - 2 * r}a$r,$r 0 0 1 $r,$r" + "v${h - 2 * r}a$r,$r 0 0 1 ${-r},$r" +
            "h${-(w - 2 * r)}a$r,$r 0 0 1 ${-r},${-r}" + "v${-(h - 2 * r)}a$r,$r 0 0 1 $r,${-r}z"

    private fun circle(cx: Float, cy: Float, r: Float): String =
        "M${cx - r},${cy}a$r,$r 0 1 0 ${2 * r},0a$r,$r 0 1 0 ${-2 * r},0z"

    private fun icon(name: String, stroke: List<String>, fill: List<String> = emptyList(), width: Float = 2f): ImageVector {
        val b = ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f)
        for (d in stroke) {
            b.addPath(
                pathData = addPathNodes(d),
                fill = null,
                stroke = Ink,
                strokeLineWidth = width,
                strokeLineCap = StrokeCap.Round,
                strokeLineJoin = StrokeJoin.Round,
            )
        }
        for (d in fill) b.addPath(pathData = addPathNodes(d), fill = Ink)
        return b.build()
    }

    private fun filled(name: String, vararg d: String): ImageVector {
        val b = ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f)
        for (p in d) b.addPath(pathData = addPathNodes(p), fill = Ink)
        return b.build()
    }

    val Mic = icon("mic", listOf(rect(9f, 3f, 6f, 11f, 3f), "M5 11a7 7 0 0 0 14 0", "M12 18v3"), width = 2.2f)
    val Send = icon("send", listOf("M12 19V5", "M6 11l6-6 6 6"), width = 2.2f)
    val LockBody = icon("lock-body", listOf(rect(5f, 11f, 14f, 10f, 2.5f)), fill = listOf(circle(12f, 16f, 1.2f)))
    val LockShackle = icon("lock-shackle", listOf("M8 11V7a4 4 0 0 1 8 0v4"))
    /** Round 12's `I.lock` whole: the Settings "Privacy policy" row (the iPhone's row uses the same). */
    val Lock = icon("lock", listOf("M8 11V7a4 4 0 0 1 8 0v4", rect(5f, 11f, 14f, 10f, 2.5f)), fill = listOf(circle(12f, 16f, 1.2f)))
    val TrashLid = icon("trash-lid", listOf("M4 7h16", "M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"), width = 2.2f)
    val TrashCan = icon("trash-can", listOf("M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13"), width = 2.2f)
    val TrashCanFill = filled("trash-can-fill", "M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13z")
    val TrashBars = icon("trash-bars", listOf("M10 11v6M14 11v6"), width = 2.2f)
    val Play = filled("play", "M8 5.5v13a1 1 0 0 0 1.5.87l11-6.5a1 1 0 0 0 0-1.74l-11-6.5A1 1 0 0 0 8 5.5z")
    val Stop = filled("stop", rect(6f, 6f, 12f, 12f, 2.5f))
    val Settings = icon("settings", listOf("M4 8h9M17 8h3M4 16h3M11 16h9", circle(14.5f, 8f, 2.5f), circle(8.5f, 16f, 2.5f)))
    val Close = icon("close", listOf("M6 6l12 12M18 6L6 18"))
    /** Round 36's `I.share`: the file viewer's Share (the phone's share sheet). */
    val Share = icon("share", listOf("M12 3v12M7.5 7.5L12 3l4.5 4.5", "M6 11H5a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-8a1 1 0 0 0-1-1h-1"))
    val Check = icon("check", listOf("M5 12.5l4.5 4.5L19 7"), width = 2.4f)
    val Spinner = icon("spinner", listOf("M12 4a8 8 0 1 1-8 8"), width = 2.4f)
    val Clock = icon("clock", listOf(circle(12f, 12f, 8f), "M12 8v4l3 2"), width = 2.4f)
    val Alert = icon("alert", listOf(circle(12f, 12f, 8.5f), "M12 8v5"), fill = listOf(circle(12f, 16.2f, 0.9f)), width = 2.4f)
    val ChevronRight = icon("chevron-right", listOf("M9 6l6 6-6 6"))
    val ChevronLeft = icon("chevron-left", listOf("M15 6l-6 6 6 6"))
    val ArrowDown = icon("arrow-down", listOf("M12 5v14M6 13l6 6 6-6"))
    val Camera = icon("camera", listOf("M4 8.5A1.5 1.5 0 0 1 5.5 7H8l1.5-2h5L16 7h2.5A1.5 1.5 0 0 1 20 8.5V18a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18z", circle(12f, 13f, 3.5f)))
    val Bell = icon("bell", listOf("M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z", "M10 20a2 2 0 0 0 4 0"))
    val Link = icon("link", listOf("M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1", "M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"))
    val Mac = icon("mac", listOf(rect(3f, 4f, 18f, 12f, 2f), "M8 20h8M12 16v4"))
    val Phone = icon("phone", listOf(rect(7f, 2.5f, 10f, 19f, 2.5f), "M11 18h2"))
    val Shield = icon("shield", listOf("M12 3l7 3v5c0 5-3.5 8.5-7 10-3.5-1.5-7-5-7-10V6z"))
    val Cloud = icon("cloud", listOf("M7 18a4 4 0 0 1-.5-8A5.5 5.5 0 0 1 17 8.5a3.8 3.8 0 0 1 .5 7.5z"))
    val Spark = icon("spark", listOf("M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5L18 18M18 6l-2.5 2.5M8.5 15.5L6 18"))
    val Refresh = icon("refresh", listOf("M20 12a8 8 0 1 1-2.3-5.7", "M20 4v5h-5"))
    val Qr = icon("qr", listOf(rect(4f, 4f, 6f, 6f, 1f), rect(14f, 4f, 6f, 6f, 1f), rect(4f, 14f, 6f, 6f, 1f), "M14 14h2v2h-2zM18 14h2M14 18h2M18 18h2v2"))
    val Download = icon("download", listOf("M12 4v12M7 11l5 5 5-5M5 20h14"))
    val Life = icon("life", listOf(circle(12f, 12f, 8.5f), circle(12f, 12f, 3.5f), "M6 6l3.5 3.5M14.5 14.5L18 18M18 6l-3.5 3.5M9.5 14.5L6 18"))
    val Globe = icon("globe", listOf(circle(12f, 12f, 8.5f), "M3.5 12h17M12 3.5c3 3 3 14 0 17M12 3.5c-3 3-3 14 0 17"))
    val Flash = icon("flash", listOf("M13 3L5 13h6l-1 8 8-10h-6z"))
    // The attachments append (round 12 `attach/attach.js` `AI`).
    val Plus = icon("plus", listOf("M12 5v14M5 12h14"), width = 2.2f)
    val Image = icon("image", listOf(rect(3.5f, 4.5f, 17f, 15f, 2.5f), circle(9f, 10f, 1.8f), "M20.5 15.5l-4.8-4.8L6.5 19.5"))
    val Folder = icon("folder", listOf("M3.5 7.5a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2v7.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"))
    val Shift = icon("shift", listOf("M12 4l7 8h-4v7H9v-7H5z"))
    val Backspace = icon("backspace", listOf("M9 5h11a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H9l-6-7z", "M12 9.5l5 5M17 9.5l-5 5"))

    /** The RichOS mark, in two parts so each takes its own color: [MarkInk] and [MarkSignal]. */
    private const val MARK_INK_PATH = "M517,744h227l-85.2-120.4c-41.7-58.7-83.3-117.5-124.9-176.2c-0.7-0.9-1.3-1.9-2.3-3.4c2.6-1.3,5.1-2.5,7.6-3.7c26.2-12.9,50.3-28.8,70.8-49.7c29.9-30.6,47-67.6,53.7-109.4c8.3-52.1,2.1-102.4-22.8-149.2C601.2,57.3,539.1,13.7,455.2,1.8c-4.1-0.6-8.1-1.2-12.2-1.8C295.3,0,147.7,0,0,0c0,133.5,0,266.7,0,400c12.8-17,24.9-34.6,38.5-51c45.4-55.2,99.8-100.2,159.9-138.6c1.1-0.7,2.2-1.5,3.3-2.2c0.3-0.2,0.4-0.5,1-1.1c-3.1-4.3-6.3-8.8-9.5-13.2c-7.2-10-14.4-20-21.6-30c-3.1-4.3-4.6-9-2.1-14c2.5-4.9,7.1-6,12.2-6c93.8,0,187.6,0,281.5,0c3.5,0,7.2-0.2,9.1,3.6c2,3.8,0.2,7-1.9,10.2c-43.9,66.2-87.8,132.5-131.6,198.7c-0.6,1-1.3,1.9-2,2.9c-7.2,9.8-17.5,10.1-24.9,0.3c-12.3-16.4-24.5-32.8-36.7-49.3c-1.2-1.6-2.4-3.1-3.9-5c-6.3,4.3-12.4,8.3-18.3,12.5c-46.1,32.6-89.8,68-128.1,109.6C62.9,494.7,23.5,573.1,8,663.2c-3.6,20.6-4.9,41.6-7.2,62.5c-0.2,1.8-0.5,3.6-0.8,5.4c0,3.7,0,9.3,0,13c1-1.6,2.3-5.2,2.9-6.9c19.5-54,47.4-103.2,86.8-145.2c59.3-63.2,132.4-100.6,217.6-115.2c6.4-1.1,9.7,0.4,13.6,5.5"
    private const val MARK_SIGNAL_PATH = "M0,400c12.8-17,24.9-34.6,38.5-51c45.4-55.2,99.8-100.2,159.9-138.6c1.1-0.7,2.2-1.5,3.3-2.2c0.3-0.2,0.4-0.5,1-1.1c-3.1-4.3-6.3-8.8-9.5-13.2c-7.2-10-14.4-20-21.6-30c-3.1-4.3-4.6-9-2.1-14c2.5-4.9,7.1-6,12.2-6c93.8,0,187.6,0,281.5,0c3.5,0,7.2-0.2,9.1,3.6c2,3.8,0.2,7-1.9,10.2c-43.9,66.2-87.8,132.5-131.6,198.7c-0.6,1-1.3,1.9-2,2.9c-7.2,9.8-17.5,10.1-24.9,0.3c-12.3-16.4-24.5-32.8-36.7-49.3c-1.2-1.6-2.4-3.1-3.9-5c-6.3,4.3-12.4,8.3-18.3,12.5c-46.1,32.6-89.8,68-128.1,109.6C62.9,494.7,23.5,573.1,8,663.2c-3.6,20.6-4.9,41.6-7.2,62.5c-0.2,1.8-0.5,3.6-0.8,5.4C0,620.7,0,510.3,0,400z"

    fun mark(ink: Color, signal: Color): ImageVector =
        ImageVector.Builder("richos-mark", 24.dp, 24.dp, 744f, 744f)
            .addPath(pathData = addPathNodes(MARK_INK_PATH), fill = SolidColor(ink))
            .addPath(pathData = addPathNodes(MARK_SIGNAL_PATH), fill = SolidColor(signal))
            .build()
}
