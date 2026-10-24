package dev.richos.android.ui

import android.app.Application
import androidx.activity.OnBackPressedDispatcher
import androidx.activity.compose.LocalOnBackPressedDispatcherOwner
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeDown
import dev.richos.android.core.Action
import dev.richos.android.core.RichFile
import dev.richos.android.core.Sheet
import dev.richos.android.core.Theme
import dev.richos.android.core.UpdateNotice
import dev.richos.android.core.protocol.Row
import dev.richos.android.ui.catalog.ScreenCatalog
import dev.richos.android.ui.model.ScreenModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The screens driven by touch, headless: a tap reaches core as the same [Action] the command line
 * sends, and the conversation follows the newest message unless the reader scrolls up — with
 * sending always resuming it (PRD §5; adoption ledger §2.8, T3's design plus RichOS's send rule).
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], application = Application::class, qualifiers = "w360dp-h640dp-xhdpi")
class InteractionTest {
    @get:Rule
    val compose = createComposeRule()

    private val events = mutableListOf<UiEvent>()
    private lateinit var backDispatcher: OnBackPressedDispatcher
    private val actions get() = events.mapNotNull { it.toAction() }

    private fun show(model: ScreenModel): (ScreenModel) -> Unit {
        var current by mutableStateOf(model)
        compose.setContent {
            backDispatcher = checkNotNull(LocalOnBackPressedDispatcherOwner.current).onBackPressedDispatcher
            RichApp(current, onEvent = { events += it })
        }
        compose.waitForIdle()
        return { current = it }
    }

    private fun screen(id: String) = ScreenCatalog.model(id, Theme.DARK, 360f)

    @Test
    fun `typing and sending reach core as compose and send`() {
        val set = show(screen("comp-idle"))
        compose.onNodeWithTag("message-field").performTextInput("Hello Rich")
        assertEquals(Action.Compose("Hello Rich"), actions.last())
        // The draft is core's: show it back as core would, and the circle becomes Send.
        set(screen("comp-typing"))
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Send message").performClick()
        assertEquals(1, actions.count { it == Action.Send })
    }

    @Test
    fun `try now, discard, the six words and forget reach core`() {
        val set = show(screen("conv-retry"))
        compose.onNodeWithText("Try now").performClick()
        assertEquals(Action.Retry, actions.last())

        set(screen("conv-pending"))
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Discard this unsent message").performClick()
        assertEquals(Action.Discard("mobile-3"), actions.last())

        set(screen("pair-words"))
        compose.waitForIdle()
        compose.onNodeWithText("They match").performClick()
        assertEquals(Action.ConfirmWords(true), actions.last())
        compose.onNodeWithText("They do not match").performClick()
        assertEquals(Action.ConfirmWords(false), actions.last())

        set(screen("settings-forget"))
        compose.waitForIdle()
        compose.onNodeWithText("Forget pairing on this phone").performClick()
        assertEquals(Action.ConfirmForget, actions.last())

        // Forget refused while a message waits: the way out when that Mac is gone.
        set(screen("settings-forget-blocked"))
        compose.waitForIdle()
        compose.onNodeWithText("Discard and forget").performClick()
        assertEquals(Action.DiscardAndForget, actions.last())
    }

    @Test
    fun `waiting for the press on the Mac - the words stay, They match is gone, They do not match reaches core`() {
        show(screen("pair-waiting-mac"))
        compose.onNodeWithText("Now press They match on your Mac").assertIsDisplayed()
        compose.onNodeWithText("They match").assertDoesNotExist()
        for (w in ScreenCatalog.model("pair-words", Theme.DARK, 360f).app.pairing.words) compose.onNodeWithText(w).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("They do not match").assertIsDisplayed().performClick()
        assertEquals(Action.ConfirmWords(false), actions.last())
    }

    /**
     * Urban's final words for the pairing cards (richos-hq `docs/verification/2026-09-24-native-pair-v2/
     * urban-review.md`, states 4 to 9) and Sage's sentence on the update card (pair-v2 hypotheses
     * review §3), the same text the PWA and the iPhone carry, with "scan it again" for the PWA's
     * "open it on this phone again".
     */
    private val cards = linkedMapOf(
        "pair-mac-update" to ("Your Mac needs an update" to
            "This phone cannot pair with the version of RichOS on it. Your Mac may say the six words did not match: it stopped because this phone did. Update RichOS on your Mac, then show a fresh code there and scan it again."),
        "pair-mac-declined" to ("Your Mac did not accept this phone" to
            "Nothing was paired. Either someone said the words did not match on your Mac, or pairing was stopped there. Show a fresh code on your Mac and scan it again."),
        "pair-expired" to ("Pairing timed out" to
            "This phone did not hear back from your Mac in time, so it stopped. Show a fresh code on your Mac and scan it again."),
        "pair-words-rejected" to ("Stopped, and nothing was paired" to
            "If the words on this phone and your Mac were different, this phone was not talking to your Mac. Tell Rich on your Mac before you pair again."),
        "pair-refused" to ("Your Mac did not accept this code" to "Nothing was paired. Show a fresh code on your Mac and scan it again."),
        "pair-fault" to ("Pairing did not finish" to "Nothing was paired. Show a fresh code on your Mac and scan it again."),
    )

    @Test
    fun `each pairing outcome says what happened in Urban's words, and the way back is the scanner`() {
        var set: ((ScreenModel) -> Unit)? = null
        for ((id, card) in cards) {
            if (set == null) set = show(screen(id)) else { set(screen(id)); compose.waitForIdle() }
            compose.onNodeWithText(card.first).performScrollTo().assertIsDisplayed()
            compose.onNodeWithText(card.second).performScrollTo().assertIsDisplayed()
            // One way back, the same verb as the card's: no second button, no "Pair again" (Urban, question 1).
            compose.onNodeWithText("Pair again").assertDoesNotExist()
            compose.onNodeWithText("Scan your Mac’s code").performClick()
            assertEquals(UiEvent.ScanCode, events.last())
            // Never the paired phone's takeover: nobody removed this phone from the Mac.
            compose.onNodeWithText("This phone was removed from your Mac").assertDoesNotExist()
        }
    }

    @Test
    fun `the words-rejected card is still there after the scanner opens and closes without a scan`() {
        val rejected = screen("pair-words-rejected")
        val set = show(rejected.copy(pairingSurface = dev.richos.android.ui.model.PairingSurface.SCANNING))
        compose.onNodeWithTag("scanner").assertIsDisplayed()
        set(rejected)
        compose.waitForIdle()
        compose.onNodeWithText("Stopped, and nothing was paired").performScrollTo().assertIsDisplayed()
    }

    @Test
    fun `at the largest text on the small phone, every pairing card's title shows without scrolling`() {
        var current by mutableStateOf(screen(cards.keys.first()))
        compose.setContent {
            val base = androidx.compose.ui.platform.LocalDensity.current
            androidx.compose.runtime.CompositionLocalProvider(androidx.compose.ui.platform.LocalDensity provides androidx.compose.ui.unit.Density(base.density, 2f)) {
                RichApp(current, onEvent = { events += it })
            }
        }
        for ((id, card) in cards) {
            current = screen(id)
            compose.waitForIdle()
            // Where the title IS, unclipped: boundsInRoot is clipped by the scrolling region, so a
            // title scrolled wholly out of view reports an empty rectangle and would pass (it did).
            val node = compose.onNodeWithText(card.first).fetchSemanticsNode()
            val top = node.positionInRoot.y
            val bottom = top + node.size.height
            val button = compose.onNodeWithText("Scan your Mac’s code").fetchSemanticsNode().positionInRoot.y
            val fade = compose.onAllNodesWithTag("scroll-edge-fade").fetchSemanticsNodes().firstOrNull()?.positionInRoot?.y
            val limit = fade ?: button
            assertTrue("$id: the card title (top $top, bottom $bottom) must sit above the scroll edge ($limit) at 2x text", bottom <= limit)
            // The positive probe: the same measure finds the title inside the frame at all.
            assertTrue("$id: the card title is laid out on the screen (top $top)", top >= 0f && node.size.height > 0)
        }
    }

    /**
     * D05: on the Tailscale route with the phone's own Tailscale off, the one nameplate line names
     * the fix, calmly (no pulsing dot: nothing here is reconnecting by itself), and the queued
     * message stays on screen. One line, never a dialog or a card that asks for a tap.
     */
    @Test
    fun `with Tailscale off, the one connection line names the fix, still, and the queued message stays`() {
        show(screen("conn-tailscale-off"))
        val line = compose.onNodeWithTag("connection-line").assertIsDisplayed().fetchSemanticsNode()
        val text = line.config[androidx.compose.ui.semantics.SemanticsProperties.Text].single().text
        assertEquals("This phone is not on Tailscale. Turn Tailscale on to reach your Mac. Messages stay on this phone.", text)
        compose.onNodeWithText("Reconnecting…", substring = true).assertDoesNotExist()
        compose.onNodeWithText("Move the Friday review to 3 PM.").performScrollTo().assertIsDisplayed()
        compose.onAllNodesWithTag("connection-line").fetchSemanticsNodes().let { assertEquals(1, it.size) }
    }

    @Test
    fun `the six words never tell the answer, and the waiting screen names both places and the control`() {
        val set = show(screen("pair-words"))
        compose.onNodeWithText("Your Mac shows six words too. If they are the same six, this connection is private to you.").assertIsDisplayed()
        set(screen("pair-waiting-mac"))
        compose.waitForIdle()
        val lede = "This phone carries on by itself once you do. If the words on your Mac are different, press They do not match, here or on your Mac."
        val node = compose.onNodeWithText(lede).assertIsDisplayed().fetchSemanticsNode()
        val text = node.config[androidx.compose.ui.semantics.SemanticsProperties.Text].single()
        val bold = text.spanStyles.single { it.item.fontWeight == androidx.compose.ui.text.font.FontWeight.SemiBold }
        assertEquals("They do not match", text.text.substring(bold.start, bold.end))
    }

    @Test
    fun `every Settings row that leaves the app reaches core - updates, support and the privacy policy`() {
        val set = show(screen("settings"))
        // No appearance control: the app follows the phone (the CEO, 2026-09-24, G9).
        compose.onNodeWithText("Appearance").assertDoesNotExist()
        // Nothing has checked for an update, so the row claims nothing: no "Up to date".
        compose.onNodeWithText("Up to date").assertDoesNotExist()
        compose.onNodeWithText("Check for updates").performScrollTo().performClick()
        assertEquals(Action.CheckForUpdates, actions.last())
        compose.onNodeWithText("Support").performScrollTo().performClick()
        assertEquals(Action.OpenSupport, actions.last())
        compose.onNodeWithText("Privacy policy").performScrollTo().assertIsDisplayed().performClick()
        assertEquals(Action.OpenPrivacyPolicy, actions.last())
        // The consent screen's "Learn more" is the policy too.
        set(screen("pair-consent"))
        compose.waitForIdle()
        compose.onNodeWithText("Learn more").performClick()
        assertEquals(Action.OpenPrivacyPolicy, actions.last())
    }

    @Test
    fun `Where your messages go opens from Settings and its Continue closes it again`() {
        val set = show(screen("settings"))
        compose.onNodeWithText("Where your messages go").performScrollTo().performClick()
        assertEquals(Action.OpenSheet(Sheet.WHERE_MESSAGES_GO), actions.last())
        set(screen("settings").let { it.copy(app = it.app.copy(sheet = Sheet.WHERE_MESSAGES_GO)) })
        compose.waitForIdle()
        compose.onNodeWithText("Continue").performClick()
        assertEquals(Action.CloseSheet, actions.last())
    }

    @Test
    fun `the update row names a version only when the hosted policy announced one`() {
        val posed = screen("settings").app
        val announced = posed.copy(update = UpdateNotice(UpdateNotice.Prominence.BANNER, "1.1", "Faster voice messages."))
        show(ScreenModel(app = announced))
        compose.onNodeWithText("1.1 is available").performScrollTo().assertIsDisplayed()
    }

    @Test
    fun `the Settings button asks core for the sheet, and the scrim asks core to close it`() {
        val set = show(screen("comp-idle"))
        compose.onNodeWithTag("message-field").performClick().assertIsFocused()
        compose.onNodeWithTag("settings-button").performClick()
        compose.onNodeWithTag("message-field").assertIsNotFocused()
        assertEquals(Action.OpenSheet(Sheet.SETTINGS), actions.last())
        // Core opens it; the screen follows core.
        set(screen("settings"))
        compose.waitForIdle()
        compose.onNodeWithTag("settings-sheet").assertIsDisplayed()
        // The sheet covers the middle of the scrim; press the scrim itself, as TalkBack would.
        compose.onNodeWithContentDescription("Close").performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.OnClick)
        assertEquals(Action.CloseSheet, actions.last())
    }

    /**
     * Found on the Honor: after a send the keyboard stayed up, and a tapped file card opened its
     * viewer squeezed above it. The tap closes the keyboard, as the Settings button does.
     */
    @Test
    fun `tapping a file from Rich closes the keyboard before its viewer opens`() {
        val idle = screen("comp-idle")
        val file = Row(
            id = "r-file", threadId = "general", cursor = 202609220834, role = "rich", kind = "file",
            createdAt = "2026-09-22T08:34:00.000Z",
            file = RichFile(id = "f-lease", name = "Lease amendment.pdf", ext = "PDF", meta = "PDF · 184 KB"),
        )
        show(idle.copy(app = idle.app.copy(messages = idle.app.messages + file)))
        compose.onNodeWithTag("message-field").performClick().assertIsFocused()
        compose.onNodeWithTag("rich-file").performScrollTo().performClick()
        compose.onNodeWithTag("message-field").assertIsNotFocused()
        assertEquals(Action.OpenRichFile("f-lease"), actions.last())
    }

    @Test
    fun `system Back dismisses Settings instead of leaving the conversation`() {
        show(screen("settings"))
        compose.runOnIdle { backDispatcher.onBackPressed() }
        assertEquals(listOf(UiEvent.CloseOverlay), events)
    }

    @Test
    fun `the way in to pairing - Scan, the link, the camera dialog, the scanner and the link sheet`() {
        val set = show(screen("pair-intro"))
        compose.onNodeWithText("Scan your Mac’s code").performClick()
        assertEquals(UiEvent.ScanCode, events.last())
        compose.onNodeWithText("Use a pairing link instead").performClick()
        assertEquals(Action.OpenSheet(Sheet.PAIRING_LINK), actions.last())

        set(screen("pair-camera-denied"))
        compose.waitForIdle()
        compose.onNodeWithText("Open Settings").performClick()
        assertEquals(Action.OpenSystemSettings, actions.last())
        compose.onNodeWithText("Use a pairing link").performClick()
        assertEquals(UiEvent.UsePairingLink, events.last())

        set(screen("pair-scanner"))
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Close the scanner").performClick()
        assertEquals(UiEvent.CloseScanner, events.last())
        compose.onNodeWithText("Use a pairing link instead").performClick()
        assertEquals(UiEvent.UsePairingLink, events.last())

        // Core opened the sheet: what is pasted reaches core as pair, exactly as typed.
        set(screen("pair-link"))
        compose.waitForIdle()
        compose.onNodeWithTag("pairing-link-sheet").assertIsDisplayed()
        val pasted = "https://mac-7f3a.example.ts.net/#pair=Zm9yLXRlc3Q"
        compose.onNodeWithTag("pairlink-field").performTextInput(pasted)
        compose.onNodeWithText("Pair with this link").performClick()
        assertEquals(UiEvent.PairWithLink(pasted), events.last())
        assertEquals(Action.Pair(pasted), actions.last())

        // Refused: core's sentence shows under the field.
        set(screen("pair-link-refused"))
        compose.waitForIdle()
        compose.onNodeWithText("Pairing requires an HTTPS origin").assertIsDisplayed()

        // Unsent work in the way while still paired, scanning another Mac: keep the pairing, or
        // send it first.
        set(screen("pair-blocked"))
        compose.waitForIdle()
        compose.onNodeWithText("Keep this pairing").performClick()
        assertEquals(Action.CloseSheet, actions.last())
        compose.onNodeWithText("Send it first").performClick()
        assertEquals(Action.Retry, actions.last())
        compose.onNodeWithText("Discard and pair").performClick()
        assertEquals(UiEvent.DiscardAndPair, events.last())

        // After "Pair again" from "removed from your Mac": nothing can be sent (UX audit G2), so
        // discard and pair, or not now (core's close; the messages stay).
        set(screen("pair-removed-blocked"))
        compose.waitForIdle()
        compose.onNodeWithText("Not now").performClick()
        assertEquals(Action.CloseSheet, actions.last())
        compose.onNodeWithText("Discard and pair").performClick()
        assertEquals(UiEvent.DiscardAndPair, events.last())
    }

    @Test
    fun `the microphone-off card - Open Settings and Not now reach core (D03)`() {
        show(screen("rec-mic-denied"))
        compose.onNodeWithText("The microphone is off for RichConnect").assertIsDisplayed()
        compose.onNodeWithText("Open Settings").performClick()
        assertEquals(Action.OpenSystemSettings, actions.last())
        compose.onNodeWithText("Not now").performClick()
        assertEquals(Action.DismissMicrophoneCard, actions.last())
        assertEquals(Action.AskMicrophone, UiEvent.MicrophoneAskAgain.toAction())
    }

    @Test
    fun `the voice gesture reaches core as press, move and release with the round-12 width`() {
        show(screen("comp-idle"))
        compose.onNodeWithTag("orb").performTouchInput {
            down(center)
            moveBy(androidx.compose.ui.geometry.Offset(-120f, 0f))
            up()
        }
        val voice = actions.filter { it is Action.VoicePress || it is Action.VoiceMove || it is Action.VoiceRelease }
        assertTrue("press first: $voice", voice.first() is Action.VoicePress)
        assertTrue("release last: $voice", voice.last() is Action.VoiceRelease)
        val press = voice.first() as Action.VoicePress
        assertTrue("the composer's width in dp (was ${press.width})", press.width in 300.0..360.0)
        val move = voice.filterIsInstance<Action.VoiceMove>().last()
        assertTrue("left travel in dp (was ${move.dx})", move.dx < -20.0)
    }

    @Test
    fun `reaching older history invokes core once and loading failure does not spin`() {
        val base = screen("conv-populated").let { it.copy(app = it.app.copy(olderAvailable = true)) }
        val update = show(base)
        repeat(12) {
            if (Action.LoadOlder !in actions) {
                compose.onNodeWithTag("thread").performTouchInput { swipeDown(startY = centerY - 200f, endY = centerY + 600f) }
                compose.waitForIdle()
            }
        }
        assertTrue("the scroll edge must reach the core action", Action.LoadOlder in actions)
        val count = actions.count { it == Action.LoadOlder }
        update(base.copy(app = base.app.copy(loadingOlder = true)))
        compose.waitForIdle()
        update(base)
        compose.waitForIdle()
        assertEquals("a failed page must not restart itself", count, actions.count { it == Action.LoadOlder })
    }

    @Test
    fun `the conversation follows new messages until the reader scrolls up, and sending resumes it`() {
        val base = screen("conv-populated")
        val update = show(base)
        fun latestShown() = compose.onAllNodesWithTagCount("latest-pill") > 0

        // Following: a new reply lands and the newest stays in view, no Latest pill.
        var rows = base.app.messages + reply("n1", "A new reply.")
        update(base.copy(app = base.app.copy(messages = rows)))
        compose.waitForIdle()
        compose.onNodeWithText("A new reply.").assertIsDisplayed()
        assertTrue(!latestShown())

        // The reader scrolls up: following stops and the Latest pill appears.
        compose.onNodeWithTag("thread").performTouchInput { swipeDown(startY = centerY - 200f, endY = centerY + 600f) }
        compose.waitForIdle()
        assertTrue("Latest pill after scrolling up", latestShown())

        // Another reply does not move the reader.
        rows = rows + reply("n2", "Another reply, while you read.")
        update(base.copy(app = base.app.copy(messages = rows)))
        compose.waitForIdle()
        assertTrue("still reading older", latestShown())

        // Sending resumes following (RichOS's rule; T3 does not do this).
        update(base.copy(app = base.app.copy(messages = rows, draft = "On my way.")))
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Send message").performClick()
        compose.waitForIdle()
        assertTrue("following again after a send", !latestShown())
        compose.onNodeWithText("Another reply, while you read.").assertIsDisplayed()
    }

    private fun reply(id: String, text: String) = Row(
        id = id, threadId = "general", cursor = 99_000_000_000L + id.hashCode().toLong(), role = "rich", text = text,
        createdAt = "2026-09-22T09:40:00.000Z",
    )

    private fun androidx.compose.ui.test.junit4.ComposeContentTestRule.onAllNodesWithTagCount(tag: String): Int =
        onAllNodes(androidx.compose.ui.test.hasTestTag(tag)).fetchSemanticsNodes().size
}
