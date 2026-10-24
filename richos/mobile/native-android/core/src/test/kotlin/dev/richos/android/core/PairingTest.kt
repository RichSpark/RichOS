package dev.richos.android.core

import dev.richos.android.core.dev.DevKeys
import dev.richos.android.core.dev.DevRequest
import dev.richos.android.core.dev.DevRuntime
import dev.richos.android.core.dev.Fixtures
import dev.richos.android.core.protocol.DeviceKeys
import dev.richos.android.core.protocol.Http
import dev.richos.android.core.protocol.HttpRequest
import dev.richos.android.core.protocol.HttpResponse
import dev.richos.android.core.protocol.MacApi
import dev.richos.android.core.protocol.Signing
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** Pairing (phone protocol contract §2–§4), headless against the scripted Mac. */
class PairingTest {
    private suspend fun unpaired() = DevRuntime.create().also { it.execute(DevRequest.Fixture("unpaired")) }

    @Test
    fun `the pairing scenarios pass`() = runTest {
        for (name in listOf("pair-and-confirm", "pair-refused", "pair-v1-mac-refused", "pair-wait-expires", "pair-mac-declined")) {
            val result = DevRuntime.create().execute(DevRequest.Scenario(name)).jsonObject
            assertEquals(name, result["name"]!!.jsonPrimitive.content)
        }
    }

    @Test
    fun `a good code sends the key and the platform, and the answer's route is the link's`() = runTest {
        val runtime = unpaired()
        val s = runtime.core.dispatch(Action.Pair(Fixtures.PAIR_LINK))
        assertEquals(PairingPhase.CONFIRMING, s.pairing.phase)
        assertEquals(Route.TAILNET, s.pairing.route)
        assertEquals(DevKeys.DEVICE_ID, s.pairing.deviceId)
        assertFalse(s.paired, "not paired until the words are confirmed")
    }

    @Test
    fun `an unreachable Mac leaves the phone unpaired with the reason, and the link can be tried again`() = runTest {
        val runtime = unpaired()
        runtime.execute(DevRequest.parse("transport", "unreachable"))
        val s = runtime.core.dispatch(Action.Pair(Fixtures.PAIR_LINK))
        assertEquals(PairingPhase.UNPAIRED, s.pairing.phase)
        assertEquals("unreachable", s.pairing.problem)
        runtime.execute(DevRequest.parse("transport", "accept"))
        assertEquals(PairingPhase.CONFIRMING, runtime.core.dispatch(Action.Pair(Fixtures.PAIR_LINK)).pairing.phase)
    }

    /** The PWA's rule (`app.js` `pair-confirm`): a lost answer is not a reason to stop, because the wait asks the Mac itself. */
    @Test
    fun `an unreachable Mac at They match still waits for the press on the Mac, and the wait's ask pairs`() = runTest {
        val runtime = unpaired()
        runtime.core.dispatch(Action.Pair(Fixtures.PAIR_LINK))
        runtime.execute(DevRequest.parse("transport", "unreachable"))
        var s = runtime.core.dispatch(Action.ConfirmWords(true))
        assertEquals(PairingPhase.AWAITING_MAC, s.pairing.phase)
        assertEquals(null, s.pairing.problem)
        assertFalse(s.paired)
        runtime.execute(DevRequest.parse("transport", "accept"))
        runtime.execute(DevRequest.parse("mac", "press"))
        s = runtime.execute(DevRequest.parse("advance", "2000")).let { runtime.core.state }
        assertEquals(PairingPhase.PAIRED, s.pairing.phase)
        assertTrue(s.paired)
    }

    /** A refusal during pairing is the Mac declining this phone, never "removed from your Mac" (which is a paired phone's takeover). */
    @Test
    fun `a revoked answer at They match is final, unpaired as declined with the key discarded`() = runTest {
        val runtime = unpaired()
        runtime.core.dispatch(Action.Pair(Fixtures.PAIR_LINK))
        runtime.execute(DevRequest.parse("transport", "revoked"))
        val s = runtime.core.dispatch(Action.ConfirmWords(true))
        assertEquals(PairingPhase.UNPAIRED, s.pairing.phase)
        assertEquals(RichCore.PROBLEM_MAC_DECLINED, s.pairing.problem)
        assertFalse(Fixtures.ORIGIN in runtime.export().keys)
    }

    /**
     * Found on the Android test phone, 2026-10-22: a phone the Mac had removed pressed Pair again,
     * the new Mac took its key, and the screen went straight back to "removed from your Mac" because
     * the connection still said revoked (stored by a health probe while the link was down). The six
     * words never showed, and the Mac's window closed unanswered. A new pairing is a fresh start.
     */
    @Test
    fun `a phone the Mac removed pairs again, and the old removal does not cover the new pairing's words`() = runTest {
        val runtime = unpaired()
        val core = runtime.core
        core.markRevoked()
        core.dispatch(Action.Link(LinkStatus.AWAY))
        var s = core.dispatch(Action.Health(phoneOnline = true))
        assertEquals(ConnectionReason.REVOKED, s.connection.reason, "the removal as the phone stood before Pair again")
        s = core.dispatch(Action.Pair(Fixtures.PAIR_LINK))
        assertEquals(PairingPhase.CONFIRMING, s.pairing.phase)
        assertTrue(s.connection.reason != ConnectionReason.REVOKED, "the new pairing is not shown as removed: ${s.connection.reason}")
    }

    @Test
    fun `They do not match on the phone while waiting tells the Mac, stops the wait and forgets the key`() = runTest {
        val runtime = unpaired()
        runtime.core.dispatch(Action.Pair(Fixtures.PAIR_LINK))
        assertEquals(PairingPhase.AWAITING_MAC, runtime.core.dispatch(Action.ConfirmWords(true)).pairing.phase)
        assertFailsWith<CoreError>("They match cannot be pressed twice") { runtime.core.dispatch(Action.ConfirmWords(true)) }
        val s = runtime.core.dispatch(Action.ConfirmWords(false))
        assertEquals(PairingPhase.UNPAIRED, s.pairing.phase)
        assertEquals(RichCore.PROBLEM_WORDS_REJECTED, s.pairing.problem, "the screen says nothing was paired (Urban's review, state 4)")
        assertEquals(null, s.macWaitDueInMs)
        assertEquals(null, runtime.export().mac.devicePoint, "the Mac forgot the phone")
        assertFalse(Fixtures.ORIGIN in runtime.export().keys)
        runtime.execute(DevRequest.parse("advance", "60000"))
        assertEquals(1, runtime.export().mac.answers, "the press was the only They match, and nothing is asked after the wait stopped")
        // The next scan clears the card: a fresh code is a fresh start.
        runtime.execute(DevRequest.parse("fixture", "unpaired"))
        assertEquals(null, runtime.core.dispatch(Action.Pair(Fixtures.PAIR_LINK)).pairing.problem)
    }

    @Test
    fun `They do not match on the six words says nothing was paired too`() = runTest {
        val runtime = unpaired()
        runtime.core.dispatch(Action.Pair(Fixtures.PAIR_LINK))
        val s = runtime.core.dispatch(Action.ConfirmWords(false))
        assertEquals(PairingPhase.UNPAIRED, s.pairing.phase)
        assertEquals(RichCore.PROBLEM_WORDS_REJECTED, s.pairing.problem)
        assertTrue(s.pairing.words.isEmpty())
        assertFalse(Fixtures.ORIGIN in runtime.export().keys)
        assertEquals(0, runtime.export().mac.answers, "no They match was ever sent")
    }

    @Test
    fun `the scenario grammar knows the Mac's press`() {
        assertEquals(DevRequest.Mac("press"), DevRequest.parse("mac", "press"))
        assertFailsWith<CoreError> { DevRequest.parse("mac", "pressed") }
        assertFailsWith<CoreError> { DevRequest.parse("mac", null) }
    }

    @Test
    fun `unsent work blocks both pairing and forgetting, with a sentence, and changes nothing`() = runTest {
        val runtime = DevRuntime.create().also { it.execute(DevRequest.Fixture("queued")) }
        val before = runtime.core.state
        assertTrue(assertFailsWith<CoreError> { runtime.core.dispatch(Action.Pair(Fixtures.PAIR_LINK)) }.message!!.contains("Send it or discard it"))
        assertTrue(assertFailsWith<CoreError> { runtime.core.dispatch(Action.Forget) }.message!!.contains("Send it or discard it"))
        assertEquals(before, runtime.core.state)
    }

    @Test
    fun `forget clears the pairing and the key`() = runTest {
        val runtime = DevRuntime.create().also { it.execute(DevRequest.Fixture("online")) }
        val s = runtime.core.dispatch(Action.Forget)
        assertEquals(PairingPhase.UNPAIRED, s.pairing.phase)
        assertFalse(s.paired)
        assertTrue(s.threads.isEmpty())
        assertFalse(Fixtures.ORIGIN in runtime.export().keys)
    }

    @Test
    fun `a bad link is refused before anything changes`() = runTest {
        val runtime = unpaired()
        val before = runtime.core.state
        assertFailsWith<CoreError> { runtime.core.dispatch(Action.Pair("http://mac.example/#pair=K7M2QX9H")) }
        assertEquals(before, runtime.core.state)
    }

    @Test
    fun `confirming with nothing to confirm is refused`() = runTest {
        assertFailsWith<CoreError> { unpaired().core.dispatch(Action.ConfirmWords(true)) }
    }

    /** The 404 recovery (contract §4.3): re-sign ONCE with the fresh challenge; a second 404 is final. */
    @Test
    fun `a 404 with a fresh challenge is re-signed once, and only once`() = runTest {
        val keys = object : DeviceKeys {
            override suspend fun publicPoint(origin: String) = DevKeys.point
            override suspend fun sign(origin: String, data: ByteArray) = DevKeys.sign(data)
            override suspend fun delete(origin: String) = Unit
        }
        val seen = mutableListOf<String>()
        fun macWith(answers: List<Int>): Http {
            var n = 0
            return Http { request: HttpRequest ->
                val credential = request.headers.getValue("Authorization")
                seen += credential.split('.')[1]
                val status = answers[n++]
                HttpResponse(status, mapOf("x-richos-challenge" to "fresh-$n"), if (status == 200) """{"ok":true}""".toByteArray() else ByteArray(0))
            }
        }
        val signed = MacApi(macWith(listOf(404, 200)), keys).confirm(Fixtures.ORIGIN, DevKeys.DEVICE_ID, "stale", true)
        assertEquals(listOf("stale", "fresh-1"), seen)
        assertEquals("fresh-2", signed.challenge)

        seen.clear()
        val failure = assertFailsWith<TransportFailure> {
            MacApi(macWith(listOf(404, 404, 200)), keys).confirm(Fixtures.ORIGIN, DevKeys.DEVICE_ID, "stale", true)
        }
        assertEquals("refused", failure.reason)
        assertFalse(failure.retryable)
        assertEquals(2, seen.size, "never a third attempt")
    }

    @Test
    fun `failures are classified as the reference client does`() = runTest {
        val keys = object : DeviceKeys {
            override suspend fun publicPoint(origin: String) = DevKeys.point
            override suspend fun sign(origin: String, data: ByteArray) = DevKeys.sign(data)
            override suspend fun delete(origin: String) = Unit
        }
        suspend fun outcome(status: Int, body: String): TransportFailure = assertFailsWith {
            MacApi(Http { HttpResponse(status, emptyMap(), body.toByteArray()) }, keys)
                .signed(Fixtures.ORIGIN, DevKeys.DEVICE_ID, "c", "GET", "/api/audio/x", null)
        }
        assertEquals("revoked" to false, outcome(403, """{"revoked":true}""").let { it.reason to it.retryable })
        assertEquals("refused" to false, outcome(403, "").let { it.reason to it.retryable })
        assertEquals("rate-limited" to true, outcome(429, "").let { it.reason to it.retryable })
        assertEquals("refused" to false, outcome(409, """{"retry":false,"reason":"reused"}""").let { it.reason to it.retryable })
        assertEquals("fault" to true, outcome(409, """{"retry":"false"}""").let { it.reason to it.retryable })
        assertEquals("fault" to true, outcome(503, """{"accepted":false,"reason":"disk"}""").let { it.reason to it.retryable })
        val unreachable = assertFailsWith<TransportFailure> {
            MacApi(Http { throw IOException("no route") }, keys).signed(Fixtures.ORIGIN, DevKeys.DEVICE_ID, "c", "GET", "/x", null)
        }
        assertEquals("unreachable" to true, unreachable.reason to unreachable.retryable)
    }

    @Test
    fun `the route follows the origin`() {
        assertEquals(Route.TAILNET, RichCore.routeOf("https://mm1.tail1a2b3c.ts.net:8443"))
        assertEquals(Route.CONNECT, RichCore.routeOf("https://c-5de0dbe0862dd461ae305af5cef35202-g2.richos.ceo"))
        assertEquals(null, RichCore.routeOf("https://mac.example"))
        assertEquals(DevKeys.DEVICE_ID, Signing.deviceId(DevKeys.point))
    }
}
