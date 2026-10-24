package dev.richos.android.core.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** One Server-Sent Events frame: `id: <cursor>\nevent: <name>\ndata: <one line of JSON>\n\n`. */
data class SseFrame(val id: String?, val event: String, val data: String)

/** What a parse step yields: frames, and the comments the Mac uses as signals. */
sealed interface SseItem {
    data class Frame(val frame: SseFrame) : SseItem

    /** `: keep-alive <ms>` — the socket is alive and idle. */
    data object KeepAlive : SseItem

    /** `: re-snapshot <n>` — the socket fell behind and ends now; reconnect WITHOUT `since`. */
    data class Resnapshot(val dropped: Long?) : SseItem
}

/**
 * An incremental SSE parser (phone protocol contract §5.4): bytes arrive split at any boundary,
 * including inside a line or a UTF-8 sequence, and the parser keeps what is not yet a whole line.
 * Lines end in `\n`, `\r\n` or `\r`; one leading space after `:` is stripped; a blank line ends a
 * frame; multiple `data` lines join with `\n`; the default event name is `message`.
 */
class SseParser {
    private val pending = java.io.ByteArrayOutputStream()
    private var id: String? = null
    private var event: String? = null
    private val data = StringBuilder()
    private var hasData = false
    private var lastWasCR = false

    fun feed(bytes: ByteArray): List<SseItem> {
        val out = mutableListOf<SseItem>()
        for (b in bytes) {
            val c = b.toInt() and 0xff
            if (c == '\n'.code && lastWasCR) {
                lastWasCR = false
                continue
            }
            lastWasCR = c == '\r'.code
            if (c == '\n'.code || c == '\r'.code) {
                line(String(pending.toByteArray(), Charsets.UTF_8), out)
                pending.reset()
            } else {
                pending.write(c)
            }
        }
        return out
    }

    fun feed(text: String): List<SseItem> = feed(text.toByteArray(Charsets.UTF_8))

    private fun line(line: String, out: MutableList<SseItem>) {
        if (line.isEmpty()) {
            if (hasData) out += SseItem.Frame(SseFrame(id, event ?: "message", data.toString()))
            event = null
            data.setLength(0)
            hasData = false
            return
        }
        if (line.startsWith(":")) {
            val comment = line.drop(1).trim()
            when {
                comment.startsWith("keep-alive") -> out += SseItem.KeepAlive
                comment.startsWith("re-snapshot") -> out += SseItem.Resnapshot(comment.removePrefix("re-snapshot").trim().toLongOrNull())
            }
            return
        }
        val colon = line.indexOf(':')
        val field = if (colon < 0) line else line.substring(0, colon)
        var value = if (colon < 0) "" else line.substring(colon + 1)
        if (value.startsWith(" ")) value = value.substring(1)
        when (field) {
            "id" -> id = value
            "event" -> event = value
            "data" -> {
                if (hasData) data.append('\n')
                data.append(value)
                hasData = true
            }
        }
    }
}

/** A conversation row (contract §5.4, `phone/rows.rs`). The Mac's row always wins a merge by [id]. */
@Serializable
data class Row(
    val id: String,
    @SerialName("thread_id") val threadId: String,
    val cursor: Long,
    val role: String,
    val question: dev.richos.android.core.QuestionCard? = null,
    val kind: String = "text",
    val text: String = "",
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("has_audio") val hasAudio: Boolean = false,
    @SerialName("from_microphone") val fromMicrophone: Boolean = false,
    val state: String = "complete",
    val complete: Boolean = true,
    /** A phone voice note's length (Echo 4ce79d6e): on hello and backfill rows only; absent means "show no length". */
    @kotlinx.serialization.EncodeDefault(kotlinx.serialization.EncodeDefault.Mode.NEVER)
    @SerialName("duration_ms") val durationMs: Long? = null,
    /**
     * This phone's own id for the message the row is, when this phone sent it (`phone/rows.rs`
     * `client_id_of`); null for everything else and on an older Mac. [Echoes] matches on it first.
     */
    @kotlinx.serialization.EncodeDefault(kotlinx.serialization.EncodeDefault.Mode.NEVER)
    @SerialName("client_id") val clientId: String? = null,
    /** `kind: "file"`: a file Rich put in front of the user (round 36 screen 3); null otherwise. */
    @kotlinx.serialization.EncodeDefault(kotlinx.serialization.EncodeDefault.Mode.NEVER)
    val file: dev.richos.android.core.RichFile? = null,
)

@Serializable
data class Hello(
    val challenge: String? = null,
    @SerialName("thread_id") val threadId: String? = null,
    @SerialName("latest_cursor") val latestCursor: Long = 0,
    val threads: List<dev.richos.android.core.ConversationThread> = emptyList(),
    val capabilities: List<String> = emptyList(),
    val build: String? = null,
    @SerialName("protocol_version") val protocolVersion: Long? = null,
    @SerialName("attachment_limits") val attachmentLimits: dev.richos.android.core.AttachmentLimits? = null,
    val messages: List<Row> = emptyList(),
)

@Serializable
data class Delta(
    @SerialName("message_id") val messageId: String,
    /** Which conversation the streaming row is in (Echo e9b0a89e); older Macs omit it. */
    @SerialName("thread_id") val threadId: String? = null,
    val cursor: Long,
    val text: String,
)

/** The answer to a backfill request, `GET /api/events?…&before=&limit=` (contract §5.5). */
@Serializable
data class Backfill(val messages: List<Row> = emptyList(), val more: Boolean = false)
