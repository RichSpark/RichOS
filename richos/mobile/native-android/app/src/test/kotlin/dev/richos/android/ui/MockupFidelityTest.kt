package dev.richos.android.ui

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.unit.Density
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onRoot
import dev.richos.android.core.Theme
import dev.richos.android.design.RichColors
import org.junit.Assert.assertEquals
import dev.richos.android.ui.catalog.ScreenCatalog
import dev.richos.android.ui.model.ScreenModel
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The app against round 12.1, the approved spec: one test per gap in Urban's 2026-09-24 audit
 * (richos-hq `docs/verification/2026-09-24-richconnect-ux-audit.md`), each on the small phone
 * (360 × 640 dp) where the gaps were seen, rendered headless (Robolectric, native graphics).
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], application = Application::class, qualifiers = "w360dp-h640dp-xhdpi")
class MockupFidelityTest {
    @get:Rule
    val compose = createComposeRule()

    private data class Shown(val model: ScreenModel, val fontScale: Float)

    private var shown by mutableStateOf<Shown?>(null)
    private var started = false
    val events = mutableListOf<UiEvent>()

    /** Shows [model] at [fontScale]; the content is set once and follows [shown] after that. */
    private fun show(model: ScreenModel, fontScale: Float = 1f) {
        shown = Shown(model, fontScale)
        if (!started) {
            started = true
            compose.setContent {
                val s = shown ?: return@setContent
                val base = LocalDensity.current
                CompositionLocalProvider(LocalDensity provides Density(base.density, s.fontScale)) {
                    key(s) { RichApp(s.model, onEvent = { events += it }) }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun screen(id: String, theme: Theme = Theme.DARK) = ScreenCatalog.model(id, theme, 360f)

    private fun nodes(): List<SemanticsNode> =
        compose.onAllNodes(SemanticsMatcher("any") { true }, useUnmergedTree = true).fetchSemanticsNodes()

    private fun text(node: SemanticsNode): String? = node.config.getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text }

    private fun layoutOf(node: SemanticsNode): TextLayoutResult? {
        val action = node.config.getOrNull(SemanticsActions.GetTextLayoutResult) ?: return null
        val out = mutableListOf<TextLayoutResult>()
        action.action?.invoke(out)
        return out.firstOrNull()
    }

    private fun node(text: String): SemanticsNode =
        nodes().firstOrNull { text(it) == text } ?: throw AssertionError("“$text” is not on screen")

    private fun hasTag(tag: String) = nodes().any { it.config.getOrNull(SemanticsProperties.TestTag) == tag }

    /**
     * G3: the consent screen (Apple 5.1.2(i)) at 360 dp and 100% text shows all three details, the
     * last one ("Encrypted on the way, stored nowhere.") with at least 18 dp of air above Continue,
     * with nothing to scroll. At 200% it scrolls, and the scroll edge is marked by the fade.
     */
    @Test
    fun `G3 - consent is readable in full on the small phone`() {
        for (theme in listOf(Theme.DARK, Theme.LIGHT)) {
            show(screen("pair-consent", theme))
            val density = compose.density.density
            val continueTop = node("Continue").boundsInRoot.top
            for (detail in listOf("Your conversation lives there, not on our servers.", "So, all the AI work happens on your Mac.", "Encrypted on the way, stored nowhere.")) {
                val b = node(detail).boundsInRoot
                val laid = layoutOf(node(detail))!!.size.height
                assertTrue("$theme: “$detail” is cut (shown ${b.height} of $laid px)", b.height + 1f >= laid)
                val air = (continueTop - b.bottom) / density
                assertTrue("$theme: “$detail” sits ${"%.1f".format(air)} dp above Continue (floor 18)", air >= 18f - 0.5f)
            }
            assertTrue("$theme: nothing to scroll at 100%, so no fade", !hasTag("scroll-edge-fade"))
        }
        show(screen("pair-consent"), fontScale = 2f)
        assertTrue("at 200% the content scrolls and its edge is marked", hasTag("scroll-edge-fade"))
    }

    private fun tagged(tag: String): SemanticsNode =
        nodes().firstOrNull { it.config.getOrNull(SemanticsProperties.TestTag) == tag } ?: throw AssertionError("no node tagged $tag")

    private fun pixel(image: android.graphics.Bitmap, x: Float, y: Float) = androidx.compose.ui.graphics.Color(image.getPixel(x.toInt(), y.toInt()))

    /**
     * The ground as it is under the speckle (design/Speckle.kt): the per-channel median of a
     * 21-pixel run of the row, so the sparse single-pixel points drop out and what is left is the
     * ground, its lamp and whatever line is drawn along that row.
     */
    private fun rowMedian(image: android.graphics.Bitmap, x: Float, y: Float): androidx.compose.ui.graphics.Color {
        val run = (-10..10).map { image.getPixel(x.toInt() + it, y.toInt()) }
        fun med(shift: Int) = run.map { (it shr shift) and 0xFF }.sorted()[run.size / 2]
        return androidx.compose.ui.graphics.Color(red = med(16), green = med(8), blue = med(0))
    }

    private fun near(a: androidx.compose.ui.graphics.Color, b: androidx.compose.ui.graphics.Color, tolerance: Float = 3f / 255f) =
        kotlin.math.abs(a.red - b.red) <= tolerance && kotlin.math.abs(a.green - b.green) <= tolerance && kotlin.math.abs(a.blue - b.blue) <= tolerance

    /**
     * G7: the CEO's paragraph on the empty conversation is in full ink (not ink-soft), kept to a
     * 300 dp measure, under a `line-faint` hairline 20 dp above it, and clears the composer at 100%.
     */
    @Test
    fun `G7 - the CEO's paragraph is full ink under a hairline`() {
        for (theme in listOf(Theme.DARK, Theme.LIGHT)) {
            val colors = if (theme == Theme.DARK) RichColors.Dark else RichColors.Light
            show(screen("conv-empty", theme))
            val density = compose.density.density
            val p = tagged("mic-how")
            val style = layoutOf(p)!!.layoutInput.style
            assertEquals("$theme: the paragraph's color", colors.ink, style.color)
            assertTrue("$theme: measure ${p.boundsInRoot.width / density} dp > 300", p.boundsInRoot.width / density <= 300.5f)
            // The hairline: the ground washed with line-faint, 20 dp above the paragraph, 1 dp tall.
            val image = compose.onRoot().captureToImage().asAndroidBitmap()
            val x = p.boundsInRoot.center.x
            val lineY = p.boundsInRoot.top - 20f * density + 0.5f * density
            // Both sampled as the median of their row, clear of the speckle's single-pixel points.
            val ground = rowMedian(image, x, lineY - 6f * density)
            val expected = colors.lineFaint.compositeOver(ground)
            val seen = rowMedian(image, x, lineY)
            assertTrue("$theme: hairline pixel $seen, expected $expected over ground $ground", near(seen, expected) && !near(seen, ground, 0f))
            // It clears the composer: nothing to scroll at 100% on the small phone.
            assertTrue("$theme: the empty state scrolls at 100%", !hasTag("scroll-edge-fade"))
            assertTrue("$theme: the paragraph ends above the composer", p.boundsInRoot.bottom <= tagged("composer").boundsInRoot.top)
        }
    }

    /** G8: the CEO's paragraph keeps his straight apostrophe, verbatim (round 12.1 NOTES, screen 12). */
    @Test
    fun `G8 - the CEO's straight apostrophe, verbatim`() {
        val verbatim = "Press and hold the gold microphone to record a voice message. Release to send. Or slide left to cancel. Or slide up to lock. Because then you don't need to hold and can scroll."
        show(screen("conv-empty"))
        assertEquals(verbatim, text(tagged("mic-how")))
        // A real empty hello has no older page. It still needs the first-use instructions.
        show(ScreenModel(app = screen("conv-empty").app.copy(olderAvailable = false)))
        assertEquals(verbatim, text(tagged("mic-how")))
    }

    /**
     * G16: at 200% text on the small phone the empty conversation scrolls, and a fade marks its
     * bottom edge above the composer, so the cut paragraph reads as a scroll edge, not clipping.
     */
    @Test
    fun `G16 - the empty conversation marks its scroll edge at 200 percent`() {
        for (theme in listOf(Theme.DARK, Theme.LIGHT)) {
            show(screen("conv-empty", theme), fontScale = 2f)
            val fade = tagged("scroll-edge-fade").boundsInRoot
            val composer = tagged("composer").boundsInRoot
            assertTrue("$theme: the fade ${fade} sits above the composer $composer", fade.bottom <= composer.top)
            assertTrue("$theme: the fade overlaps the paragraph's cut", fade.overlaps(tagged("mic-how").boundsInRoot))
        }
    }

    private fun descendants(node: SemanticsNode): List<SemanticsNode> = node.children.flatMap { listOf(it) + descendants(it) }

    private fun under(tag: String, merged: Boolean): List<SemanticsNode> {
        val root = compose.onAllNodes(SemanticsMatcher("any") { true }, useUnmergedTree = !merged).fetchSemanticsNodes()
            .firstOrNull { it.config.getOrNull(SemanticsProperties.TestTag) == tag } ?: throw AssertionError("no node tagged $tag")
        return descendants(root)
    }

    /** The labels of the controls inside the node tagged [tag] (merged tree, as TalkBack reads them). */
    private fun controlsIn(tag: String): List<String> =
        under(tag, merged = true).filter { it.config.getOrNull(SemanticsActions.OnClick) != null }
            .map { n -> (n.config.getOrNull(SemanticsProperties.Text)?.joinToString(" ") { it.text }).orEmpty() }

    /** The texts inside the node tagged [tag], in reading order. */
    private fun textsIn(tag: String): List<String> = under(tag, merged = false).mapNotNull { text(it) }

    /**
     * G2 (audit §4.1): after the Mac removed this phone, "Pair again" meeting unsent work offers no
     * Send (it cannot succeed) and names no pairing (there is none): Urban's copy, exactly two
     * buttons. Still paired, `pair-blocked` is unchanged.
     */
    @Test
    fun `G2 - after removal the dialog offers discard and pair, or not now`() {
        val one = screen("pair-removed-blocked")
        show(one)
        assertEquals(listOf("Discard and pair", "Not now"), controlsIn("pairing-blocked-dialog"))
        assertEquals(
            listOf(
                "One message is still waiting",
                "It was written for the Mac that removed this phone, so it can’t be sent now. Discard it, then pair again.",
                "Discard and pair", "Not now",
            ),
            textsIn("pairing-blocked-dialog"),
        )

        val waiting = one.app.outbox.first()
        val two = one.copy(app = one.app.copy(outbox = listOf(waiting, waiting.copy(clientId = "mobile-2", text = "And cancel the car."))))
        show(two)
        assertEquals(
            listOf(
                "Two messages are still waiting",
                "They were written for the Mac that removed this phone, so they can’t be sent now. Discard them, then pair again.",
                "Discard and pair", "Not now",
            ),
            textsIn("pairing-blocked-dialog"),
        )

        show(screen("pair-blocked"))
        assertEquals(listOf("Send it first", "Discard and pair", "Keep this pairing"), controlsIn("pairing-blocked-dialog"))
        assertTrue(textsIn("pairing-blocked-dialog").any { "paired with now" in it })
    }

    /** The refused Forget dialog's sentence agrees with its count: one message is "it", more are "them". */
    @Test
    fun `the refused Forget dialog says it for one message and them for two`() {
        val two = screen("settings-forget-blocked")
        show(two)
        assertTrue(textsIn("forget-blocked-dialog").contains(
            "Two messages are still waiting to be sent. Send them or discard them first, then this phone can forget the pairing.",
        ))
        show(two.copy(app = two.app.copy(outbox = two.app.outbox.take(1))))
        assertTrue(textsIn("forget-blocked-dialog").contains(
            "One message is still waiting to be sent. Send it or discard it first, then this phone can forget the pairing.",
        ))
    }

    /**
     * G13: the disabled orb (`comp-disabled`, `conn-incompatible`) is round 12.1's gold circle dimmed
     * to 45% over the capsule, not an outlined ring, and stays disabled for TalkBack.
     */
    @Test
    fun `G13 - the disabled orb is dimmed gold`() {
        for (id in listOf("comp-disabled", "conn-incompatible")) {
            for (theme in listOf(Theme.DARK, Theme.LIGHT)) {
                val colors = if (theme == Theme.DARK) RichColors.Dark else RichColors.Light
                show(screen(id, theme))
                val density = compose.density.density
                val orb = tagged("orb")
                assertTrue("$id $theme: the orb is disabled", orb.config.getOrNull(SemanticsProperties.Disabled) != null)
                val image = compose.onRoot().captureToImage().asAndroidBitmap()
                val c = orb.boundsInRoot.center
                // Inside the 44 dp circle, clear of the 22 dp glyph: the fill, not an empty ring.
                val fill = pixel(image, c.x - 16f * density, c.y)
                val expected = colors.signal.copy(alpha = 0.45f).compositeOver(colors.surface)
                assertTrue("$id $theme: orb fill $fill, expected dimmed gold $expected", near(fill, expected, 4f / 255f))
            }
        }
    }

    /**
     * G14: on the large phone a waiting voice message keeps "0:14 · Waiting to send · Now" on one
     * line (`conv-pending`), as round 12.1 and iOS do. On the small phone the row may wrap, as the
     * mockup's own `.vmeta { flex-wrap: wrap }` does there.
     */
    @Test @Config(qualifiers = "w412dp-h915dp-xhdpi")
    fun `G14 - the voice bubble's status shares the duration's line`() {
        for (theme in listOf(Theme.DARK, Theme.LIGHT)) {
            show(ScreenCatalog.model("conv-pending", theme, 412f))
            val density = compose.density.density
            val duration = node("0:14").boundsInRoot
            val status = node("Waiting to send").boundsInRoot
            assertTrue("$theme: status $status is not on the duration's line $duration", kotlin.math.abs(status.center.y - duration.center.y) <= 4f * density)
            assertTrue("$theme: status sits after the duration", status.left > duration.right)
        }
    }

    /**
     * G12: over a live camera the scanner is dimmed from the first frame, while the camera is still
     * opening, and once it shows, the captions sit on the dim (never on the bare feed) with the
     * viewfinder's window left clear. The feed here is a white frame, the worst case.
     */
    @Test
    fun `G12 - the scanner is dimmed from the first frame`() {
        var status by mutableStateOf(dev.richos.android.ui.model.ScannerCamera.CHECKING)
        compose.setContent {
            dev.richos.android.design.RichTheme(Theme.LIGHT) {
                dev.richos.android.ui.overlays.Scanner(
                    found = false, onEvent = {}, status = status,
                    camera = { androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.fillMaxSize().background(androidx.compose.ui.graphics.Color.White)) },
                )
            }
        }
        compose.waitForIdle()
        val density = compose.density.density
        val dimmed = RichColors.Fixed.cameraDim.compositeOver(androidx.compose.ui.graphics.Color.White)
        var image = compose.onRoot().captureToImage().asAndroidBitmap()
        val mid = image.width / 2f
        assertTrue("opening: the feed is dimmed", hasTag("camera-dim") && near(pixel(image, mid, image.height * 0.2f), dimmed))
        assertTrue("opening: the viewfinder area is dimmed too", near(pixel(image, mid, image.height * 0.45f), dimmed))

        status = dev.richos.android.ui.model.ScannerCamera.READY
        compose.waitForIdle()
        image = compose.onRoot().captureToImage().asAndroidBitmap()
        val caption = node("Point the camera at the code on your Mac.").boundsInRoot
        assertTrue("showing: the caption sits on the dim", near(pixel(image, caption.center.x, caption.top - 2f), dimmed))
        assertTrue("showing: the viewfinder's window is clear", near(pixel(image, mid, image.height * 0.45f), androidx.compose.ui.graphics.Color.White))
    }

    /** G6: the serif headings are never hyphenated ("RichCon-nect", "re-moved"), at any text size. */
    @Test
    fun `G6 - headings are never hyphenated`() {
        for ((id, scale) in listOf("upd-dialog" to 1f, "conn-revoked" to 1f, "upd-blocking" to 1f, "upd-dialog" to 2f, "conn-revoked" to 2f)) {
            show(screen(id, Theme.LIGHT), scale)
            val headings = nodes().filter { it.config.getOrNull(SemanticsProperties.Heading) != null && text(it) != null }
            assertTrue("$id: a heading is on screen", headings.isNotEmpty())
            for (h in headings) {
                val layout = layoutOf(h) ?: continue
                assertTrue("$id at ${scale}x: “${text(h)}” is set to hyphenate", layout.layoutInput.style.hyphens != Hyphens.Auto)
            }
        }
    }
}
