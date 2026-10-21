package dev.richos.android.core

import dev.richos.android.core.dev.DevKeys
import dev.richos.android.core.dev.Fixtures
import dev.richos.android.core.protocol.DeviceKeys
import dev.richos.android.core.protocol.Http
import dev.richos.android.core.protocol.HttpRequest
import dev.richos.android.core.protocol.HttpResponse
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A file Rich put in front of the user, on the phone (round 36 screen 3): the Mac's `file` row, a
 * tap that brings the bytes over on ONE signed `GET /api/messages?kind=file`, the copy kept for the
 * next tap, the Mac's "gone" said, the wait while the Mac is out of reach, and × stopping it with
 * nothing half-written kept.
 */
class RichFileTest {
    private val pdf = "%PDF-1.4 counteroffer".toByteArray()
    private val requests = mutableListOf<HttpRequest>()
    private var status = 200

    /** The phone's kept files, in memory: what was finished, and what was abandoned. */
    private class Kept : RichFileStore {
        val files = mutableMapOf<String, ByteArray>()
        var abandoned = 0
        override suspend fun kept(id: String, name: String): String? = if (id in files) "/kept/$id/$name" else null
        override suspend fun begin(id: String, name: String): RichFileWriter = object : RichFileWriter {
            val out = java.io.ByteArrayOutputStream()
            override suspend fun write(bytes: ByteArray) = out.write(bytes)
            override suspend fun finish(): String { files[id] = out.toByteArray(); return "/kept/$id/$name" }
            override suspend fun abandon() { abandoned++ }
        }
    }

    private fun core(kept: Kept, online: Boolean = true, download: EventStream? = null): RichCore {
        var saved: Session = Fixtures.fixture("online").session
        val keys = object : DeviceKeys {
            override suspend fun publicPoint(origin: String) = DevKeys.point
            override suspend fun sign(origin: String, data: ByteArray) = DevKeys.sign(data)
            override suspend fun delete(origin: String) = Unit
        }
        val mac = Http { request ->
            requests += request
            if (status == 200) HttpResponse(200, mapOf("content-type" to "application/pdf"), pdf)
            else HttpResponse(status, emptyMap(), """{"retry":false,"reason":"No longer on this Mac. It was moved or deleted."}""".toByteArray())
        }
        val core = runBlocking {
            RichCore.open(
                Ports(
                    storage = object : OutboxStorage {
                        override suspend fun all() = emptyList<OutboxItem>()
                        override suspend fun put(item: OutboxItem) = Unit
                        override suspend fun remove(clientId: String) = Unit
                    },
                    session = object : SessionStore {
                        override suspend fun read() = saved
                        override suspend fun write(session: Session) { saved = session }
                    },
                    transport = null, clock = { Fixtures.EPOCH }, ids = { "x" }, http = mac, keys = keys,
                    richFiles = kept, download = download,
                ),
            )
        }
        val hello = "id: 2\nevent: hello\ndata: {\"challenge\":\"C2\",\"thread_id\":\"general\",\"threads\":[{\"id\":\"general\",\"title\":\"General\"}]," +
            "\"capabilities\":[\"text\",\"files\"],\"messages\":[" +
            """{"id":"t1:text:0","thread_id":"general","cursor":1,"role":"rich","kind":"text","text":"Here it is.","created_at":"2023-11-14T22:13:20.000Z","complete":true},""" +
            """{"id":"rf_1","thread_id":"general","cursor":2,"role":"rich","kind":"file","text":"Acme-counteroffer.pdf","created_at":"2023-11-14T22:13:20.000Z","complete":true,""" +
            """"file":{"id":"rf_1","name":"Acme-counteroffer.pdf","ext":"PDF","type":"PDF","meta":"PDF · 21 bytes","bytes":21,"exists":true}}""" +
            "]}\n\n"
        runBlocking {
            core.dispatch(Action.Receive(hello))
            if (!online) core.dispatch(Action.Link(LinkStatus.AWAY))
        }
        return core
    }

    @Test
    fun `a tap brings the file over on one signed request, keeps it, and a second tap opens it at once`() {
        val kept = Kept()
        val core = core(kept)
        assertEquals("the row carries the card", "PDF · 21 bytes", core.state.messages.single { it.kind == "file" }.file?.meta)
        runBlocking { core.dispatch(Action.OpenRichFile("rf_1")) }
        val asked = requests.single()
        assertEquals("GET", asked.method)
        assertEquals(Fixtures.ORIGIN + "/api/messages?kind=file&file_id=rf_1&thread_id=general", asked.url)
        assertTrue("signed in the header", asked.headers["Authorization"].orEmpty().startsWith("RichOS-Device ${DevKeys.DEVICE_ID}.C2."))
        val fetch = core.state.fileFetches.getValue("rf_1")
        assertEquals(FileFetchState.KEPT, fetch.state)
        assertTrue("the viewer opens", fetch.open)
        assertArrayEquals(pdf, kept.files.getValue("rf_1"))

        runBlocking { core.dispatch(Action.RichFileViewed("rf_1")) }
        assertEquals(false, core.state.fileFetches.getValue("rf_1").open)
        runBlocking { core.dispatch(Action.OpenRichFile("rf_1")) }
        assertEquals("the kept copy opens without asking the Mac", 1, requests.size)
        assertTrue(core.state.fileFetches.getValue("rf_1").open)
    }

    @Test
    fun `a file the Mac says is gone says so, and nothing half-written is kept`() {
        status = 410
        val kept = Kept()
        val core = core(kept)
        runBlocking { core.dispatch(Action.OpenRichFile("rf_1")) }
        val fetch = core.state.fileFetches.getValue("rf_1")
        assertEquals(FileFetchState.FAILED, fetch.state)
        assertEquals(RichFileRule.GONE, fetch.reason)
        assertTrue(kept.files.isEmpty())
        assertTrue(kept.abandoned >= 1)
    }

    @Test
    fun `with the Mac out of reach it waits and asks nothing, then comes over when asked again online`() {
        val kept = Kept()
        val core = core(kept, online = false)
        runBlocking { core.dispatch(Action.OpenRichFile("rf_1")) }
        assertEquals(FileFetchState.WAITING, core.state.fileFetches.getValue("rf_1").state)
        assertTrue(requests.isEmpty())
        runBlocking {
            core.dispatch(Action.Link(LinkStatus.OPEN))
            core.dispatch(Action.OpenRichFile("rf_1"))
        }
        assertEquals(FileFetchState.KEPT, core.state.fileFetches.getValue("rf_1").state)
    }

    @Test
    fun `the ring fills as the bytes arrive, and x stops it with nothing kept`() {
        val kept = Kept()
        val rest = CompletableDeferred<Unit>()
        val stream = EventStream { _, onOpen, onBytes ->
            onOpen(200)
            onBytes(ByteArray(40_000))
            rest.await()
            onBytes(ByteArray(40_000))
        }
        val core = core(kept, download = stream)
        runBlocking {
            val tap = async { core.dispatch(Action.OpenRichFile("rf_1")) }
            while (core.state.fileFetches["rf_1"]?.received != 40_000L) yield()
            assertEquals(FileFetchState.COMING, core.state.fileFetches.getValue("rf_1").state)
            assertEquals("40 KB from your Mac", RichFileRule.progress(40_000, null))
            core.dispatch(Action.StopRichFile("rf_1"))
            runCatching { tap.await() }
        }
        assertNull("× leaves nothing behind", core.state.fileFetches["rf_1"])
        assertTrue(kept.files.isEmpty())
        assertTrue(kept.abandoned >= 1)
        assertTrue("nothing went over the plain request", requests.isEmpty())
    }
}
