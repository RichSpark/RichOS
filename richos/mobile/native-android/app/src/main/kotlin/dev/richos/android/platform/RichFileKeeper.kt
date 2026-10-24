package dev.richos.android.platform

import android.content.Context
import android.content.Intent
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import dev.richos.android.core.RichFileStore
import dev.richos.android.core.RichFileWriter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream

/**
 * The files Rich attached, once they came over from the Mac (round 36 screen 3): kept in
 * app-private storage, `files/rich-files/<id>/<name>`, so a second tap opens one at once (the
 * round's README: "The phone fetches the file from the Mac only when tapped, and keeps it after").
 * A file is written beside its final name and renamed into place only when every byte arrived, so
 * a stopped or failed transfer never looks kept.
 */
class RichFileKeeper(private val dir: File) : RichFileStore {
    override suspend fun kept(id: String, name: String): String? = withContext(Dispatchers.IO) {
        target(id, name)?.takeIf { it.isFile }?.path
    }

    override suspend fun begin(id: String, name: String): RichFileWriter? = withContext(Dispatchers.IO) {
        val target = target(id, name) ?: return@withContext null
        target.parentFile?.mkdirs()
        val part = File(target.path + ".part")
        val out = FileOutputStream(part)
        object : RichFileWriter {
            override suspend fun write(bytes: ByteArray) = withContext(Dispatchers.IO) { out.write(bytes) }

            override suspend fun finish(): String = withContext(Dispatchers.IO) {
                out.close()
                if (!part.renameTo(target)) throw java.io.IOException("the file could not be kept")
                target.path
            }

            override suspend fun abandon() {
                withContext(Dispatchers.IO) {
                    runCatching { out.close() }
                    part.delete()
                }
            }
        }
    }

    /** `<dir>/<id>/<name>`, or null for an id or a name that would leave its folder. */
    private fun target(id: String, name: String): File? {
        if (!safe(id) || !safe(name)) return null
        return File(File(dir, id), name)
    }

    private fun safe(part: String) = part.isNotEmpty() && part.length <= 255 && !part.contains('/') && !part.contains('\\') &&
        part != "." && part != ".." && !part.contains('\u0000')

    companion object {
        fun dir(context: Context) = File(context.filesDir, "rich-files")
    }
}

/** The provider Share hands a kept file through: `files/rich-files/` and nothing else (res/xml/rich_files_paths.xml). */
class RichFilesProvider : FileProvider()

object RichFileShare {
    private const val AUTHORITY = ".richfiles"

    /** The phone's share sheet for a kept file: Save to Files, Mail, Messages, … */
    fun share(context: Context, path: String, name: String) {
        val uri = uri(context, path) ?: return
        val send = Intent(Intent.ACTION_SEND).setType(type(name)).putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        runCatching { context.startActivity(Intent.createChooser(send, name).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }

    /**
     * A kept file of a kind the app does not show itself (a Word document, a slide deck, …), opened
     * in the app on this phone that opens that kind, read-only. False when no app here opens it.
     */
    fun view(context: Context, path: String, name: String): Boolean {
        val uri = uri(context, path) ?: return false
        val open = Intent(Intent.ACTION_VIEW).setDataAndType(uri, type(name))
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        return runCatching { context.startActivity(open) }.isSuccess
    }

    private fun uri(context: Context, path: String) =
        runCatching { FileProvider.getUriForFile(context, context.packageName + AUTHORITY, File(path)) }.getOrNull()

    private fun type(name: String) =
        MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', "").lowercase()) ?: "application/octet-stream"
}
