package dev.richos.android.app

import android.content.Context
import android.util.AtomicFile
import dev.richos.android.core.AttachPicker
import dev.richos.android.core.Clock
import dev.richos.android.core.CoreJson
import dev.richos.android.core.IdSource
import dev.richos.android.core.OutboxItem
import dev.richos.android.core.OutboxStorage
import dev.richos.android.core.Ports
import dev.richos.android.core.Session
import dev.richos.android.core.SessionStore
import dev.richos.android.core.FileStore
import dev.richos.android.core.Platform
import dev.richos.android.core.Recorder
import dev.richos.android.platform.HttpsMac
import dev.richos.android.platform.KeystoreKeys
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import java.io.File
import java.io.IOException
import java.util.UUID

/**
 * The production ports: JSON files in app-private storage, each replaced atomically
 * (build plan §3.3 "JSON files in app-private storage with AtomicFile; no Room").
 * Messages go to the paired Mac through the core's own transport over [Http] and [DeviceKeys].
 */
object AppPorts {
    /**
     * [wire] is the HTTPS connection to the Mac (requests and the event stream); the caller keeps
     * it to hand the same instance to the connection owner.
     */
    fun create(context: Context, wire: HttpsMac = HttpsMac(), recorder: Recorder = Recorder.NONE, platform: Platform = Platform.NONE, picker: AttachPicker = AttachPicker.NONE): Ports {
        val dir = File(context.filesDir, "core")
        val outboxFile = JsonFile(File(dir, "outbox.json"), ListSerializer(OutboxItem.serializer())) { emptyList() }
        return Ports(
            storage = object : OutboxStorage {
                override suspend fun all(): List<OutboxItem> = outboxFile.read()
                override suspend fun put(item: OutboxItem) = outboxFile.write(outboxFile.read().filter { it.clientId != item.clientId } + item)
                override suspend fun remove(clientId: String) = outboxFile.write(outboxFile.read().filter { it.clientId != clientId })
            },
            session = LocalSessionStore(dir),
            // Null: the core speaks to the paired Mac itself over [http] and [keys] (MacTransport).
            transport = null,
            clock = Clock { System.currentTimeMillis() },
            ids = IdSource { "android-" + UUID.randomUUID() },
            http = wire,
            keys = KeystoreKeys(),
            deviceName = "Android phone",
            applicationId = context.packageName,
            performance = dev.richos.android.core.PerformanceEvents(PerformanceMarks::mark),
            files = StagedFiles(stagedDir(context)),
            recorder = recorder,
            platform = platform,
            picker = picker,
            // The files Rich attached, kept once they came over, and read in parts so the ring fills.
            richFiles = dev.richos.android.platform.RichFileKeeper(dev.richos.android.platform.RichFileKeeper.dir(context)),
            download = wire,
        )
    }

    /** Where recordings and attachments are written before they are sent (and kept after). */
    fun stagedDir(context: Context) = File(context.filesDir, "staged")

}

/**
 * Recordings and attachments the platform has written, by id, in app-private storage. An id is
 * a file name inside [dir] and nothing else: a path separator or a dot-dot never leaves it.
 */
class StagedFiles(private val dir: File) : FileStore {
    override suspend fun bytes(id: String): ByteArray? = withContext(Dispatchers.IO) {
        if (!safe(id)) return@withContext null
        File(dir, id).takeIf { it.isFile }?.readBytes()
    }

    /** A photo or file removed from the tray, discarded, or accepted by the Mac: gone from the phone. */
    override suspend fun delete(id: String) {
        withContext(Dispatchers.IO) { if (safe(id)) File(dir, id).delete() }
    }

    private fun safe(id: String) = id.isNotEmpty() && !id.contains('/') && !id.contains('\\') && id != "." && id != ".."
}

/**
 * One value in one file. Unreadable user work is preserved in place and reported to the UI.
 * Only a genuinely missing file is a fresh default, including after a process restart.
 */
class JsonFile<T>(private val file: File, private val serializer: KSerializer<T>, private val default: () -> T) {
    private val atomic = AtomicFile(file)
    private var validated = false

    /** Fields a newer build added are skipped, not a reason to set the whole file aside. */
    private val json = Json(from = CoreJson) { ignoreUnknownKeys = true }

    suspend fun read(): T = withContext(Dispatchers.IO) {
        if (!file.exists() && !File(file.path + ".bak").exists()) return@withContext default()
        try {
            json.decodeFromString(serializer, String(atomic.readFully(), Charsets.UTF_8)).also { validated = true }
        } catch (e: SerializationException) {
            throw IOException("Saved data could not be read. It has been preserved. Contact support before resetting the app.", e)
        } catch (e: IllegalArgumentException) {
            throw IOException("Saved data could not be read. It has been preserved. Contact support before resetting the app.", e)
        }
    }

    suspend fun write(value: T) = withContext(Dispatchers.IO) {
        if (!validated) read() // Never overwrite an unreadable file, even before the first read.
        file.parentFile?.mkdirs()
        val stream = atomic.startWrite()
        try {
            stream.write(CoreJson.encodeToString(serializer, value).toByteArray(Charsets.UTF_8))
            atomic.finishWrite(stream)
        } catch (e: Exception) {
            atomic.failWrite(stream)
            throw e
        }
    }

}
