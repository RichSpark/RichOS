package dev.richos.android.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Everything the core needs from the outside world, injected (richos/mobile/AGENTS.md:
// "Inject storage, transport, clock and platform operations"). The same five ports as the
// preserved phone core's `createApp(ports)`. The app supplies Android implementations, the
// headless CLI and the emulator bridge supply the deterministic ones in `dev/`.

/** The durable outbox: every queued message, surviving process death. */
interface OutboxStorage {
    suspend fun all(): List<OutboxItem>

    suspend fun put(item: OutboxItem)

    suspend fun remove(clientId: String)
}

/** The durable [Session]: draft, selected conversation, pairing, theme. */
interface SessionStore {
    suspend fun read(): Session

    suspend fun write(session: Session)
}

fun interface Clock {
    /** Milliseconds since the epoch. */
    fun now(): Long
}

fun interface IdSource {
    /** A fresh client id for a queued message. */
    fun next(): String
}

/** The Mac. A failure is a [TransportFailure]; anything else is an acknowledgement. */
interface Transport {
    suspend fun sendText(item: OutboxItem): Receipt

    /** A recorded voice message. A transport without voice refuses it for this message only. */
    suspend fun sendVoice(item: OutboxItem): Receipt = throw TransportFailure("unsupported", retryable = false, aboutThisMessage = true)

    /** Photos and files: every upload, then the commit. Only the commit's 200 means accepted. */
    suspend fun sendAttachments(item: OutboxItem): Receipt = throw TransportFailure("unsupported", retryable = false, aboutThisMessage = true)
}

/**
 * The microphone and the recordings on disk (the platform's half of the voice machine). The core
 * says WHEN to start, stop, keep and delete; the platform does it. The default does nothing, so a
 * headless world without audio still runs every rule.
 */
interface Recorder {
    suspend fun play(id: String): Boolean = false

    /** Plays Rich's reply [id] from the Mac's audio bytes; false when it cannot be played. One player: [stopPlayback] ends it. */
    suspend fun playReply(id: String, audio: ByteArray): Boolean = false
    suspend fun stopPlayback() {}
    suspend fun start(id: String) {}

    /** Recover only the journaled file, repairing its header and measuring its captured duration. */
    suspend fun recover(recording: KeptRecording): KeptRecording? = null

    /** Stop recording [id]; keep the file when [keep], else delete it. */
    suspend fun stop(id: String, keep: Boolean) {}

    suspend fun delete(id: String) {}

    /** Ask the OS once; the answer comes back as the `microphone-permission` action. */
    suspend fun requestMicrophone() {}

    /** The small tick when the lock engages. */
    suspend fun haptic() {}

    companion object {
        val NONE: Recorder = object : Recorder {}
    }
}

/** The Mac's answer to an accepted message (`POST /api/messages`). */
@Serializable
data class Receipt(
    @SerialName("message_id") val messageId: String,
    val duplicate: Boolean = false,
    val cursor: Long,
    /** Voice only: the SHA-256 of the transcript the Mac made, lowercase hex (`phone/routes.rs`). */
    @SerialName("text_sha256") val textSha256: String? = null,
    val outcome: String? = null,
    val question: QuestionCard? = null,
)

/**
 * Why a send did not reach the Mac. [retryable] separates "try again later" (unreachable)
 * from a final answer that needs the user (revoked).
 */
open class TransportFailure(
    val reason: String,
    val retryable: Boolean,
    /** A final answer about THIS message only (409/422/503 with `retry:false`): the queue moves on. */
    val aboutThisMessage: Boolean = false,
    /** An attachment commit's 422 `{"missing":[ids]}`: the uploads the Mac does not hold. */
    val missing: List<String> = emptyList(),
    /**
     * The Mac is waiting for the person to press "They match" ON THE MAC (pairing v2, Sage F1):
     * a 409 `{"awaiting_mac_confirmation":true}`. A fault, retryable and never final, but named,
     * so the pairing screen can say which press is missing.
     */
    val awaitingMac: Boolean = false,
    /** The fresh challenge the refusal carried (`X-RichOS-Challenge`), when it carried one. */
    val challenge: String? = null,
) : Exception(reason)

fun interface PerformanceEvents {
    fun mark(event: String)
}

class Ports(
    val storage: OutboxStorage,
    val session: SessionStore,
    /** Null in the app: the core speaks to the paired Mac itself ([MacTransport]). A script supplies one. */
    val transport: Transport?,
    val clock: Clock,
    val ids: IdSource,
    /** The wire to the Mac, for pairing and every signed request (`protocol.MacApi`). */
    val http: dev.richos.android.core.protocol.Http,
    /** The phone's non-exportable P-256 identity, one key per paired origin. */
    val keys: dev.richos.android.core.protocol.DeviceKeys,
    /** What the Mac lists this phone as (contract §2.3: trimmed to 40 characters there). */
    val deviceName: String = "Android phone",
    val recorder: Recorder = Recorder.NONE,
    val platform: Platform = Platform.NONE,
    val files: FileStore = FileStore.NONE,
    /** Android's photo picker, camera and document picker (the + menu). */
    val picker: AttachPicker = AttachPicker.NONE,
    /** Where the files Rich attached are kept once they came over (round 36 screen 3). */
    val richFiles: RichFileStore = RichFileStore.NONE,
    /**
     * A signed GET whose answer arrives in parts, so the ring fills as the bytes come (the app's
     * `HttpsMac`). Null: the file comes in one answer through [http], and the ring jumps.
     */
    val download: EventStream? = null,
    /** The id the Mac registers push for (Echo 65952d16: `dev.richos.native.android` in development). */
    val applicationId: String = "dev.richos.native.android",
    val performance: PerformanceEvents = PerformanceEvents { },
)
