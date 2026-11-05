package app.dictate

import android.content.Context
import android.os.Handler
import android.os.Looper
import java.io.File
import java.util.concurrent.Executors

/** Where the app keeps its files. */
class AppFiles(context: Context) {
    private val app = context.applicationContext

    /** Settings (`config.toml` with the dictionary), transcripts: the app's private folder. */
    val data: File = app.filesDir

    /**
     * The speech model: the app's own folder on the shared storage, where DownloadManager can
     * write (no permission needed, removed with the app).
     */
    val modelRoot: File? = app.getExternalFilesDir(null)

    val model: File get() = File(File(modelRoot ?: data, MODELS), Native.modelName())

    val transcripts: TranscriptLog get() = TranscriptLog(File(data, "transcripts.txt"))

    companion object {
        const val MODELS = "models"
    }
}

/**
 * The speech model in this process, loaded once and kept: one background thread loads it and
 * runs every transcription in turn (keyboard and shared messages), callbacks come on the main
 * thread.
 */
object Engine {
    enum class State { IDLE, LOADING, READY, FAILED }

    private val worker = Executors.newSingleThreadExecutor { Thread(it, "dictate-engine") }
    private val main = Handler(Looper.getMainLooper())
    private var runtimeLoaded = false

    @Volatile
    var state = State.IDLE
        private set

    /** Loads the model in the background unless it is loaded; [done] gets null or the error. */
    fun prepare(context: Context, done: (String?) -> Unit = {}) {
        val files = AppFiles(context)
        worker.execute {
            val result = runCatching { ensureLoaded(files) }
            main.post { done(result.exceptionOrNull()?.let(::describe)) }
        }
    }

    /** Mono [samples] at [rate] Hz to text, after the jobs before it. */
    fun transcribe(context: Context, samples: FloatArray, rate: Int, done: (Result<String>) -> Unit) {
        val files = AppFiles(context)
        worker.execute {
            val result = runCatching {
                ensureLoaded(files)
                Native.transcribe(samples, rate)
            }
            main.post { done(result) }
        }
    }

    /** The dictionary changed: the loaded model favours the new words. */
    fun dictionaryChanged() {
        worker.execute {
            if (state == State.READY) runCatching { Native.reloadSettings() }
        }
    }

    /** On the worker. */
    private fun ensureLoaded(files: AppFiles) {
        if (state == State.READY) return
        state = State.LOADING
        try {
            if (!runtimeLoaded) {
                // dictate-core opens ONNX Runtime by name when the model loads (`ort`
                // load-dynamic); loaded here first, from the AAR's copy in the APK.
                System.loadLibrary("onnxruntime")
                runtimeLoaded = true
            }
            Native.load(files.model.path, files.data.path)
            state = State.READY
        } catch (e: Throwable) {
            state = State.FAILED
            throw e
        }
    }

    fun describe(e: Throwable): String = e.message ?: e.javaClass.simpleName
}
