package app.dictate

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import java.io.File
import java.util.Locale
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

    /** Transcribed WhatsApp voice messages, by chat (for the strip over an open chat). */
    val voiceNotes: VoiceNoteLog get() = VoiceNoteLog(File(data, "voice_notes.txt"))

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

    /** The last transcription's timing, for the app screen ("4.2 s of audio in 0.85 s"). */
    @Volatile
    var lastTiming: Timing? = null
        private set

    class Timing(val audioSeconds: Float, val millis: Long)

    /** Loads the model in the background unless it is loaded; [done] gets null or the error. */
    fun prepare(context: Context, done: (String?) -> Unit = {}) {
        val files = AppFiles(context)
        worker.execute {
            val result = runCatching { ensureLoaded(files) }
            main.post { done(result.exceptionOrNull()?.let(::describe)) }
        }
    }

    /**
     * Mono [samples] at [rate] Hz to text, after the jobs before it. [live]: a pass over the
     * recording so far, for the text while speaking (not logged, not the last timing).
     */
    fun transcribe(
        context: Context,
        samples: FloatArray,
        rate: Int,
        live: Boolean = false,
        done: (Result<String>) -> Unit,
    ) {
        val files = AppFiles(context)
        worker.execute {
            val result = runCatching {
                ensureLoaded(files)
                val started = SystemClock.elapsedRealtime()
                val text = Native.transcribe(samples, rate)
                if (live) return@runCatching text
                val timing = Timing(samples.size.toFloat() / rate, SystemClock.elapsedRealtime() - started)
                lastTiming = timing
                // `adb logcat -s Dictate` on the phone: speed of the model on this CPU.
                Log.i(
                    TAG,
                    String.format(
                        Locale.ROOT,
                        "transcribed %.1f s of audio in %d ms (%.2f x real time)",
                        timing.audioSeconds,
                        timing.millis,
                        timing.millis / 1000f / timing.audioSeconds.coerceAtLeast(0.01f),
                    ),
                )
                text
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
            val started = SystemClock.elapsedRealtime()
            Native.load(files.model.path, files.data.path)
            Log.i(TAG, "model loaded in ${SystemClock.elapsedRealtime() - started} ms")
            state = State.READY
        } catch (e: Throwable) {
            state = State.FAILED
            throw e
        }
    }

    fun describe(e: Throwable): String = e.message ?: e.javaClass.simpleName

    const val TAG = "Dictate"
}
