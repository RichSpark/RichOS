package dev.richos.android.design

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color

/**
 * The RichOS palette, one instance per appearance.
 *
 * Every value is either a ruled token (richos-hq `design/system/tokens.css`, §14 dark
 * "Sovereign", §15 light "Daybreak") or a DECLARED gap value from round 12's `NOTES.md`
 * ("Gaps — values chosen here, declared for the CEO"). Nothing else. The comments name the
 * source of each value so a reviewer can check it without opening the mockup.
 *
 * Contrast for every text pairing is computed, never eyeballed: [ContrastPairings] lists them
 * and `ContrastTest` fails the build if one drops below its WCAG AA floor.
 */
@Immutable
data class RichColors(
    val isDark: Boolean,
    /** The page. §14/§15 `ground`. */
    val ground: Color,
    /** Rich's bubbles, cards, sheet rows, the nameplate, the composer capsule. `surface`. */
    val surface: Color,
    /** All readable text. `ink`. */
    val ink: Color,
    /** The CEO's own color: the orb, primary buttons, focus. `signal`. */
    val signal: Color,
    /** Glyphs and labels on [signal]. `on-signal`. */
    val onSignal: Color,
    /** The ruled `trim`. GAP 1: a line on the dark ground, 2.94:1, declared (no control depends on it). */
    val line: Color,
    /** Hairlines on cards and the capsule: [line] at 38% dark / 34% light. GAP 1, declared exempt. */
    val lineFaint: Color,
    /** GAP 3: the secondary tone, [ink] at 72% over whatever is behind it. */
    val inkSoft: Color,
    /** GAP 5: your own bubble, the ground washed with signal at 13% dark / 12% light. */
    val mine: Color,
    /** Signal at 13% / 12%: your bubble's edge, the Cancel ripple, the second halo. */
    val signalWash: Color,
    /** Signal at 22%: the recording halo. Decoration behind the orb; declared exempt in light. */
    val signalHalo: Color,
    /** GAP 4: danger, the value already live in the app. Recording dot, bin, "Not sent", Forget. */
    val danger: Color,
    /** Behind sheets and dialogs. */
    val scrim: Color,
    /** The float shadow's color (`--sh-float`). */
    val shadow: Color,
    /** The orb's glow (`--sh-orb`). */
    val orbGlow: Color,
    /** The lamp: a whisper of light at the top of the ground (`--lamp`). */
    val lamp: Color,
    /** A drawing of the system keyboard, shown only in review frames (NOTES "the keyboard"). */
    val keyboardKey: Color,
    val keyboardGround: Color,
) {
    /** Fixed colors of surfaces that are not ours or do not follow the theme (NOTES gaps). */
    object Fixed {
        /** The scanner's dark scene and its chrome: a camera view, dark in both themes. */
        val scannerScene = Color(0xFF060A12)
        val scannerInk = Color(0xFFDFE4EE)
        val scannerInkSoft = Color(0xFFDFE4EE).copy(alpha = 0.78f)
        val scannerChrome = Color(0xFF141E34).copy(alpha = 0.85f)
        val scannerEdge = Color(0xFFDFE4EE).copy(alpha = 0.28f)
        /**
         * Over a LIVE camera, outside the viewfinder: the scene at 75%. Over a white frame (the worst
         * case) the scanner's ink measures 7.6:1 and its soft ink 5.1:1 (ContrastTest).
         */
        val cameraDim = Color(0xFF060A12).copy(alpha = 0.75f)
        /** The gold viewfinder: the dark signal, 7.36:1 on the scene (non-text, 3:1 floor). */
        val viewfinder = Color(0xFFC2A35C)
        /** The app icon's mark: the icon does not follow the theme. */
        val markInk = Color(0xFFDFE4EE)
        val markSignal = Color(0xFFC2A35C)
        val iconGround = Color(0xFF0C1322)
        /** A drawing of the system's notification shade, review frames only (NOTES "the lock screen"). */
        val shadeTop = Color(0xFF1B2A4A)
        val shadeMid = Color(0xFF0C1322)
        val shadeBottom = Color(0xFF05080F)
        val shadeInk = Color(0xFFF2F4F8)
        val shadeCard = Color(0xFFDFE4EE).copy(alpha = 0.14f)

        /** GAP A5 (attachments NOTES): the veil and disc on a photo, the same dark in both themes. */
        val photoVeil = Color(0xFF080C16).copy(alpha = 0.38f)
        val photoDisc = Color(0xFF080C16).copy(alpha = 0.80f)
        val photoDiscInk = Color(0xFFDFE4EE)
        val photoDiscArc = Color(0xFFC2A35C)
        /** Decoration, declared exempt: the unfilled track; progress is the gold arc. */
        val photoDiscTrack = Color(0xFFDFE4EE).copy(alpha = 0.26f)

        /** The photo viewer: black, with the scanner's ink. */
        val viewerGround = Color(0xFF000000)
        val viewerInk = Color(0xFFDFE4EE)
        val viewerInkSoft = Color(0xFFDFE4EE).copy(alpha = 0.78f)
        val viewerDotOff = Color(0xFFDFE4EE).copy(alpha = 0.40f)
    }

    companion object {
        /** §14 "Sovereign" — what a new install opens in (ceo-decisions §15). */
        val Dark = RichColors(
            isDark = true,
            ground = Color(0xFF0C1322),
            surface = Color(0xFF141E34),
            ink = Color(0xFFDFE4EE),
            signal = Color(0xFFC2A35C),
            onSignal = Color(0xFF0C1322),
            line = Color(0xFF4C6087),
            lineFaint = Color(0xFF4C6087).copy(alpha = 0.38f),
            inkSoft = Color(0xFFDFE4EE).copy(alpha = 0.72f),
            mine = Color(0xFF242629),
            signalWash = Color(0xFFC2A35C).copy(alpha = 0.13f),
            signalHalo = Color(0xFFC2A35C).copy(alpha = 0.22f),
            danger = Color(0xFFE8837C),
            scrim = Color(0xFF03070F).copy(alpha = 0.66f),
            shadow = Color(0xFF000000).copy(alpha = 0.70f),
            orbGlow = Color(0xFFC2A35C).copy(alpha = 0.55f),
            lamp = Color(0xFF4C6087).copy(alpha = 0.22f),
            keyboardKey = Color(0xFF2A3450),
            keyboardGround = Color(0xFF0A101B),
        )

        /** §15 "Daybreak" — a choice, never the default. */
        val Light = RichColors(
            isDark = false,
            ground = Color(0xFFEAE6DD),
            surface = Color(0xFFF7F5EF),
            ink = Color(0xFF0C1322),
            signal = Color(0xFF9C7C34),
            onSignal = Color(0xFF0C1322),
            line = Color(0xFF4C6087),
            lineFaint = Color(0xFF4C6087).copy(alpha = 0.34f),
            inkSoft = Color(0xFF0C1322).copy(alpha = 0.72f),
            mine = Color(0xFFE1D9C9),
            signalWash = Color(0xFF9C7C34).copy(alpha = 0.12f),
            signalHalo = Color(0xFF9C7C34).copy(alpha = 0.22f),
            danger = Color(0xFF8A2F28),
            scrim = Color(0xFF2B2516).copy(alpha = 0.42f),
            shadow = Color(0xFF2E2816).copy(alpha = 0.35f),
            orbGlow = Color(0xFF9C7C34).copy(alpha = 0.50f),
            lamp = Color(0xFFFFFFFF).copy(alpha = 0.55f),
            keyboardKey = Color(0xFFFDFCF8),
            keyboardGround = Color(0xFFD9D4C8),
        )

    }
}
