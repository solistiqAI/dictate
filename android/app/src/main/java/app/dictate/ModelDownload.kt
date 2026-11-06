package app.dictate

import android.app.DownloadManager
import android.content.Context
import android.net.ConnectivityManager
import android.net.Uri
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * The speech model's download through the system's DownloadManager (it goes on in the
 * background, resumes, shows a notification). Each file comes as `<name>.part` and gets its name
 * when all are complete, so a present file is whole.
 */
class ModelDownload(context: Context) {
    private val app = context.applicationContext
    private val files = AppFiles(app)
    private val manager = app.getSystemService(DownloadManager::class.java)
    private val prefs = app.getSharedPreferences("download", Context.MODE_PRIVATE)

    class Part(val url: String, val name: String)

    sealed interface Status {
        data object Idle : Status
        data class Running(val done: Long, val total: Long) : Status
        data class Failed(val reason: String) : Status
        data object Complete : Status
    }

    fun parts(): List<Part> = Native.modelFiles().toList().chunked(2).map { Part(it[0], it[1]) }

    fun installed(): Boolean = Native.modelInstalled(files.model.path)

    /** On mobile data (or another metered network): say so before 670 MB. */
    fun metered(): Boolean = app.getSystemService(ConnectivityManager::class.java)?.isActiveNetworkMetered == true

    /** The files' total size from the server; null when it does not say. Blocks: not on the main thread. */
    fun size(): Long? = runCatching {
        parts().sumOf { part ->
            val connection = URL(part.url).openConnection() as HttpURLConnection
            try {
                connection.requestMethod = "HEAD"
                connection.instanceFollowRedirects = true
                connection.connectTimeout = 10_000
                connection.readTimeout = 10_000
                connection.contentLengthLong.takeIf { it > 0 } ?: return null
            } finally {
                connection.disconnect()
            }
        }
    }.getOrNull()

    fun start() {
        cancel()
        if (files.modelRoot == null) return
        val folder = files.model.also { it.mkdirs() }
        val ids = parts().filter { !File(folder, it.name).exists() }.map { part ->
            File(folder, part.name + PART).delete()
            val request = DownloadManager.Request(Uri.parse(part.url))
                .setTitle(app.getString(R.string.download_title))
                .setDescription(part.name)
                .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE)
                .setDestinationInExternalFilesDir(app, null, "${AppFiles.MODELS}/${folder.name}/${part.name}$PART")
            manager.enqueue(request)
        }
        prefs.edit().putString(IDS, ids.joinToString(",")).apply()
    }

    fun cancel() {
        val ids = ids()
        if (ids.isNotEmpty()) manager.remove(*ids)
        prefs.edit().remove(IDS).apply()
    }

    /** Where the download stands; completes it (renames the files) when all are there. */
    fun status(): Status {
        if (installed()) return Status.Complete
        val ids = ids()
        if (ids.isEmpty()) return Status.Idle
        var done = 0L
        var total = 0L
        var finished = 0
        var found = 0
        manager.query(DownloadManager.Query().setFilterById(*ids))?.use { cursor ->
            val statusColumn = cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_STATUS)
            val reasonColumn = cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_REASON)
            val doneColumn = cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_BYTES_DOWNLOADED_SO_FAR)
            val totalColumn = cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_TOTAL_SIZE_BYTES)
            while (cursor.moveToNext()) {
                found++
                when (cursor.getInt(statusColumn)) {
                    DownloadManager.STATUS_FAILED -> {
                        cancel()
                        return Status.Failed(reason(cursor.getInt(reasonColumn)))
                    }
                    DownloadManager.STATUS_SUCCESSFUL -> finished++
                }
                done += cursor.getLong(doneColumn).coerceAtLeast(0)
                total += cursor.getLong(totalColumn).coerceAtLeast(0)
            }
        }
        if (found < ids.size) {
            // Cancelled from the notification.
            cancel()
            return Status.Idle
        }
        if (finished == ids.size) {
            prefs.edit().remove(IDS).apply()
            for (part in parts()) {
                val partial = File(files.model, part.name + PART)
                if (partial.exists()) partial.renameTo(File(files.model, part.name))
            }
            return if (installed()) Status.Complete else Status.Failed("incomplete")
        }
        return Status.Running(done, total)
    }

    private fun ids(): LongArray =
        prefs.getString(IDS, "").orEmpty().split(",").mapNotNull { it.toLongOrNull() }.toLongArray()

    private fun reason(code: Int): String = when (code) {
        DownloadManager.ERROR_INSUFFICIENT_SPACE -> "not enough storage"
        DownloadManager.ERROR_HTTP_DATA_ERROR, DownloadManager.ERROR_TOO_MANY_REDIRECTS -> "network error"
        DownloadManager.ERROR_DEVICE_NOT_FOUND -> "no storage"
        DownloadManager.ERROR_CANNOT_RESUME -> "cannot resume"
        else -> if (code in 400..599) "HTTP $code" else "error $code"
    }

    companion object {
        const val PART = ".part"
        private const val IDS = "ids"
    }
}
