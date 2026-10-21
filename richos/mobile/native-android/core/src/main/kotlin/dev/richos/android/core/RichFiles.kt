package dev.richos.android.core

import dev.richos.android.core.protocol.Signing
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * A file Rich put in front of the user (round 36 screen 3, "A file from Rich"; the Mac's
 * `phone/rows.rs` `rich_file` row and `richos-core` `rich_files.rs`). The row carries an id to ask
 * the Mac for, never a path: the bytes come over only when the user taps the file.
 */
@Serializable
data class RichFile(
    val id: String,
    val name: String,
    val ext: String = "FILE",
    val type: String = "",
    /** The card's own line, "PDF · 184 KB", as the Mac wrote it. */
    val meta: String = "",
    val bytes: Long? = null,
    /** False when the file was moved or deleted on the Mac. */
    val exists: Boolean = true,
)

/** Where one of Rich's files stands on this phone. */
@Serializable
enum class FileFetchState {
    /** Coming over from the Mac: the ring fills. */
    @SerialName("coming") COMING,

    /** Tapped while the Mac was out of reach: the clock, until it is back. */
    @SerialName("waiting") WAITING,

    /** On this phone: a tap opens it at once. */
    @SerialName("kept") KEPT,

    /** The Mac said why it could not send it; [FileFetch.reason] is its sentence. */
    @SerialName("failed") FAILED,
}

@Serializable
data class FileFetch(
    val id: String,
    val state: FileFetchState,
    /** Bytes received so far, for the ring and the line that counts them. */
    val received: Long = 0,
    val total: Long? = null,
    val reason: String? = null,
    /** Where the kept copy is on this phone, once [FileFetchState.KEPT]. */
    val path: String? = null,
    /** The user tapped it: open the viewer as soon as it is here. */
    val open: Boolean = false,
)

/** The phone's copies of Rich's files: written once, kept after (round 36 README, "keeps it after"). */
interface RichFileStore {
    /** The kept copy of file [id], or null when there is none. */
    suspend fun kept(id: String, name: String): String? = null

    /** A place to write file [id] as it arrives, or null when this phone cannot keep files. */
    suspend fun begin(id: String, name: String): RichFileWriter? = null

    companion object {
        val NONE: RichFileStore = object : RichFileStore {}
    }
}

interface RichFileWriter {
    suspend fun write(bytes: ByteArray)

    /** Everything arrived: keep it, and say where. */
    suspend fun finish(): String

    /** Stopped or failed part-way: nothing half-written is kept. */
    suspend fun abandon()
}

object RichFileRule {
    /** The Mac capability that says it sends the files Rich attached. */
    const val CAPABILITY = "files"

    /** The Mac's own sentence for a file that is gone (`opener.rs` FILE_GONE, the route's 410). */
    const val GONE = "No longer on this Mac. It was moved or deleted."
    const val TOO_LARGE = "This file is too large to send to your phone. Open it on your Mac."
    const val FAILED = "Your Mac could not send this file. Tap to try again."

    /** The signed request: the file's id and its conversation, on the message route. */
    fun path(fileId: String, threadId: String): String =
        "/api/messages?kind=file&file_id=" + Signing.encodeURIComponent(fileId) + "&thread_id=" + Signing.encodeURIComponent(threadId)

    /** The line under the name while the Mac is out of reach (round 36, "Phone: Mac is offline"). */
    const val WAITING_LINE = "Opens when your Mac is back online"

    /** "62 of 184 KB from your Mac", the line under the name while the ring fills (round 36). */
    fun progress(received: Long, total: Long?): String {
        val kb = { b: Long -> maxOf(0L, Math.round(b / 1000.0)) }
        return if (total != null && total > 0) "${kb(received)} of ${kb(total)} KB from your Mac" else "${kb(received)} KB from your Mac"
    }

    /** How far the ring is filled, 0..1; 0 when the size is not known. */
    fun fraction(f: FileFetch): Float = f.total?.takeIf { it > 0 }?.let { (f.received.toFloat() / it).coerceIn(0f, 1f) } ?: 0f
}
