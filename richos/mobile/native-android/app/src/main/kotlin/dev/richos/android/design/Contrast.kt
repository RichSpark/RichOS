package dev.richos.android.design

import androidx.compose.ui.graphics.Color
import kotlin.math.pow

/**
 * WCAG 2.x contrast, computed (CLAUDE.md "Contrast — WCAG AA, ALWAYS, BOTH THEMES": compute the
 * ratio, never eyeball it). The formula is WCAG's own: relative luminance of sRGB channels, then
 * (L1 + 0.05) / (L2 + 0.05). A foreground with alpha is composited over its background first,
 * which is how the screen shows it.
 */
object Contrast {
    fun ratio(foreground: Color, background: Color): Double {
        val bg = opaque(background)
        val fg = over(foreground, bg)
        val a = luminance(fg)
        val b = luminance(bg)
        return (maxOf(a, b) + 0.05) / (minOf(a, b) + 0.05)
    }

    /** [top] drawn over an opaque [bottom], as the screen composites it. */
    fun over(top: Color, bottom: Color): Color {
        val a = top.alpha
        return Color(
            red = top.red * a + bottom.red * (1 - a),
            green = top.green * a + bottom.green * (1 - a),
            blue = top.blue * a + bottom.blue * (1 - a),
            alpha = 1f,
        )
    }

    private fun opaque(c: Color): Color = if (c.alpha >= 1f) c else c.copy(alpha = 1f)

    fun luminance(c: Color): Double {
        fun channel(v: Float): Double {
            // Quantize to the 8-bit value the screen shows, as every contrast checker does.
            val s = Math.round(v * 255.0) / 255.0
            return if (s <= 0.04045) s / 12.92 else ((s + 0.055) / 1.055).pow(2.4)
        }
        return 0.2126 * channel(c.red) + 0.7152 * channel(c.green) + 0.0722 * channel(c.blue)
    }
}

/** What a pairing must clear. */
enum class Floor(val ratio: Double, val label: String) {
    /** Normal readable text: 4.5:1. */
    TEXT(4.5, "text 4.5"),
    /** Large text (18.66 px bold / 24 px+) and non-text indicators: 3:1. */
    LARGE_OR_INDICATOR(3.0, "indicator 3.0"),
    /** DECLARED EXEMPT: not text and no control depends on it. The reason travels with the row. */
    EXEMPT(0.0, "exempt"),
}

data class Pairing(
    val name: String,
    val foreground: Color,
    val background: Color,
    val floor: Floor,
    /** Required when [floor] is [Floor.EXEMPT]: where a reviewer sees why. */
    val exemption: String? = null,
) {
    val ratio: Double get() = Contrast.ratio(foreground, background)
    val passes: Boolean get() = floor == Floor.EXEMPT || ratio >= floor.ratio
}

/**
 * Every color pairing the screens use, per theme. Each composable draws text only from these
 * pairings; `ContrastTest` computes them all in both themes and fails on any below its floor.
 * Planes that carry alpha (your bubble's wash) are composited onto the ground first.
 */
object ContrastPairings {
    fun of(c: RichColors): List<Pairing> {
        val mine = c.mine
        val scene = RichColors.Fixed.scannerScene
        val chrome = Contrast.over(RichColors.Fixed.scannerChrome, scene)
        val shade = RichColors.Fixed.shadeMid
        val shadeCard = Contrast.over(RichColors.Fixed.shadeCard, shade)
        val discOverWhite = Contrast.over(RichColors.Fixed.photoDisc, androidx.compose.ui.graphics.Color.White)
        return listOf(
            Pairing("ink on ground", c.ink, c.ground, Floor.TEXT),
            Pairing("ink on surface (Rich's bubbles, cards, sheet rows)", c.ink, c.surface, Floor.TEXT),
            Pairing("ink on your bubble", c.ink, mine, Floor.TEXT),
            Pairing("ink-soft on ground (day marker, beginning, sheet section)", c.inkSoft, c.ground, Floor.TEXT),
            Pairing("ink-soft on surface (placeholder, hints, timestamps, status line)", c.inkSoft, c.surface, Floor.TEXT),
            Pairing("ink-soft on your bubble (timestamps, Sending…)", c.inkSoft, mine, Floor.TEXT),
            Pairing("on-signal on signal (mic/send glyph, primary buttons, play)", c.onSignal, c.signal, Floor.TEXT),
            Pairing("signal on ground (the orb's boundary, non-text)", c.signal, c.ground, Floor.LARGE_OR_INDICATOR),
            Pairing("signal on surface (the orb in the capsule, thinking dots, non-text)", c.signal, c.surface, Floor.LARGE_OR_INDICATOR),
            Pairing("danger on surface (recording dot, bin, non-text)", c.danger, c.surface, Floor.LARGE_OR_INDICATOR),
            Pairing("danger on ground (Not sent, Forget row)", c.danger, c.ground, Floor.TEXT),
            Pairing("danger on surface (Forget row inside the sheet)", c.danger, c.surface, Floor.TEXT),
            Pairing("danger on your bubble (needs-attention glyph, non-text)", c.danger, mine, Floor.LARGE_OR_INDICATOR),
            Pairing("playing voice length on your bubble (signal dark / ink light)", if (c.isDark) c.signal else c.ink, mine, Floor.TEXT),
            Pairing("disabled orb, gold dimmed to 45% (signal on surface)", c.signal.copy(alpha = 0.45f), c.surface, Floor.EXEMPT,
                "WCAG 1.4.11 exempts inactive components: round 12.1 dims the disabled orb (audit G13), and the composer's own line says why sending is off at text contrast"),
            Pairing("ink on keyboard key (review frames)", c.ink, c.keyboardKey, Floor.TEXT),
            Pairing("toggle knob off (ink 70% on ground, non-text)", c.ink.copy(alpha = 0.7f), c.ground, Floor.LARGE_OR_INDICATOR),
            Pairing("toggle track on (signal on surface, non-text)", c.signal, c.surface, Floor.LARGE_OR_INDICATOR),
            Pairing("eyebrow (signal dark / ink light) on ground",
                if (c.isDark) c.signal else c.ink, c.ground, Floor.TEXT),
            Pairing("trim line on ground", c.line, c.ground, Floor.EXEMPT,
                "GAP 1, declared in round-12 NOTES: a hairline, not text; no control is identified by a line alone"),
            Pairing("faint hairline on surface", c.lineFaint, c.surface, Floor.EXEMPT,
                "GAP 1, declared: card and capsule hairlines are decoration; each has a fill and shadow"),
            Pairing("recording halo behind the orb", c.signalHalo, c.surface, Floor.EXEMPT,
                "Declared in round-12 NOTES: decoration behind the orb, whose glyph carries the contrast"),
            Pairing("scanner caption on the scene", RichColors.Fixed.scannerInk, scene, Floor.TEXT),
            Pairing("scanner secondary caption on the scene", RichColors.Fixed.scannerInkSoft, scene, Floor.TEXT),
            Pairing("scanner button label on its chrome", RichColors.Fixed.scannerInk, chrome, Floor.TEXT),
            Pairing("gold viewfinder on the scene (non-text)", RichColors.Fixed.viewfinder, scene, Floor.LARGE_OR_INDICATOR),
            Pairing("notification text on the drawn shade card", RichColors.Fixed.shadeInk, shadeCard, Floor.TEXT),
            // Attachments (round-12 attachments NOTES): photo overlays measured over a WHITE pixel, their worst case.
            Pairing("disc glyphs and the time chip on a photo (over white)", RichColors.Fixed.photoDiscInk, discOverWhite, Floor.TEXT),
            Pairing("upload ring's gold arc on the disc (over white, non-text)", RichColors.Fixed.photoDiscArc, discOverWhite, Floor.LARGE_OR_INDICATOR),
            Pairing("upload ring's track", RichColors.Fixed.photoDiscTrack, discOverWhite, Floor.EXEMPT,
                "Declared in the attachments NOTES: the unfilled track is decoration; progress is the gold arc"),
            Pairing("viewer name and caption on black", RichColors.Fixed.viewerInk, RichColors.Fixed.viewerGround, Floor.TEXT),
            Pairing("viewer date line on black", RichColors.Fixed.viewerInkSoft, RichColors.Fixed.viewerGround, Floor.TEXT),
            Pairing("viewer album dot, inactive (non-text)", RichColors.Fixed.viewerDotOff, RichColors.Fixed.viewerGround, Floor.LARGE_OR_INDICATOR),
            Pairing("file extension on its page (ink on surface)", c.ink, c.surface, Floor.TEXT),
            Pairing("reference chip gold bar on ground (non-text)", c.signal, c.ground, Floor.LARGE_OR_INDICATOR),
            Pairing("the + glyph on the capsule (ink-soft on surface, non-text)", c.inkSoft, c.surface, Floor.LARGE_OR_INDICATOR),
            // The pairing entry (ui/pairing): the link sheet on the ground, its field on the surface.
            Pairing("pairing link typed into its field (ink on surface)", c.ink, c.surface, Floor.TEXT),
            Pairing("pairing link placeholder “Paste the link” (ink-soft on surface)", c.inkSoft, c.surface, Floor.TEXT),
            Pairing("pairing link field edge while focused (signal on ground, non-text)", c.signal, c.ground, Floor.LARGE_OR_INDICATOR),
            Pairing("pairing link field edge unfocused (trim on ground)", c.line, c.ground, Floor.EXEMPT,
                "GAP 1, as the trim line: the field is identified by its placeholder or text (4.5:1 or better) and opens focused with the signal edge"),
            Pairing("pairing link refusal sentence (ink on ground)", c.ink, c.ground, Floor.TEXT),
            Pairing("pairing link refusal glyph (danger on ground, non-text)", c.danger, c.ground, Floor.LARGE_OR_INDICATOR),
            // Pairing v2 (ui/overlays/Takeovers.kt): the wait for the press on the Mac and its three outcomes.
            Pairing("waiting for the Mac: heading and line (ink on ground)", c.ink, c.ground, Floor.TEXT),
            Pairing("waiting for the Mac: the six words (ink on surface)", c.ink, c.surface, Floor.TEXT),
            Pairing("waiting for the Mac: They do not match, the quiet button (ink on ground)", c.ink, c.ground, Floor.TEXT),
            Pairing("pairing outcome card: title (ink on surface)", c.ink, c.surface, Floor.TEXT),
            Pairing("pairing outcome card: what to do (ink-soft on surface)", c.inkSoft, c.surface, Floor.TEXT),
            Pairing("pairing outcome card: alert glyph (danger on surface, non-text)", c.danger, c.surface, Floor.LARGE_OR_INDICATOR),
            // D03: the microphone-off card, both of its forms (Allow microphone / Open Settings).
            Pairing("microphone-off card: title (ink on surface)", c.ink, c.surface, Floor.TEXT),
            Pairing("microphone-off card: what to do (ink-soft on surface)", c.inkSoft, c.surface, Floor.TEXT),
            Pairing("microphone-off card: Allow microphone / Open Settings (on-signal on signal)", c.onSignal, c.signal, Floor.TEXT),
            Pairing("microphone-off card: Not now, the quiet button (ink on surface)", c.ink, c.surface, Floor.TEXT),
            Pairing("microphone-off card: Allow microphone / Open Settings edge (signal on surface, non-text)", c.signal, c.surface, Floor.LARGE_OR_INDICATOR),
            // A live camera may show white: every word on the scanner, over the dim over a white frame.
            Pairing("scanner over a live camera: caption and title (over the dim over white)", RichColors.Fixed.scannerInk, Contrast.over(RichColors.Fixed.cameraDim, androidx.compose.ui.graphics.Color.White), Floor.TEXT),
            Pairing("scanner over a live camera: secondary caption (over the dim over white)", RichColors.Fixed.scannerInkSoft, Contrast.over(RichColors.Fixed.cameraDim, androidx.compose.ui.graphics.Color.White), Floor.TEXT),
            Pairing("scanner over a live camera: link and close chrome (over the dim over white)", RichColors.Fixed.scannerInk,
                Contrast.over(RichColors.Fixed.scannerChrome, Contrast.over(RichColors.Fixed.cameraDim, androidx.compose.ui.graphics.Color.White)), Floor.TEXT),
            Pairing("scanner: camera not available (scanner ink on the scene)", RichColors.Fixed.scannerInk, RichColors.Fixed.scannerScene, Floor.TEXT),
            Pairing("scanner: the camera opening, spinner arc (signal on the scene, non-text)", c.signal, RichColors.Fixed.scannerScene, Floor.LARGE_OR_INDICATOR),
            // The dim now covers the feed from its first frame, the spinner included; the scanner draws
            // it in the dark palette in both themes (audit G12).
            Pairing("scanner: the camera opening over a live camera, spinner arc (dark signal over the dim over white, non-text)",
                RichColors.Dark.signal, Contrast.over(RichColors.Fixed.cameraDim, androidx.compose.ui.graphics.Color.White), Floor.LARGE_OR_INDICATOR),
        )
    }
}
