package app.dictate

import android.Manifest
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.dictate.ui.DictateTheme
import app.dictate.ui.MainActions
import app.dictate.ui.MainScreen
import app.dictate.ui.MainState
import app.dictate.ui.ModelState
import app.dictate.ui.VoiceNotesState

/** The app screen: setup (microphone, model, keyboard), try it, recent transcripts, dictionary. */
class MainActivity : ComponentActivity() {
    private var state by mutableStateOf(
        MainState(false, ModelState.Missing(null, false), StartWith.BUTTON, false, false, null, emptyList(), emptyList()),
    )
    private lateinit var download: ModelDownload
    private val main = Handler(Looper.getMainLooper())
    private var modelBytes: Long? = null
    private var failure: String? = null

    private val poll = object : Runnable {
        override fun run() {
            refresh()
            if (state.model is ModelState.Downloading) main.postDelayed(this, 500)
        }
    }

    private val askMic = registerForActivityResult(ActivityResultContracts.RequestPermission()) { refresh() }

    /** Android 13+: the transcript notifications need this. */
    private val askNotifications = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        if (!Setup.notificationsAllowed(this)) Setup.openAppInfo(this)
        refresh()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        download = ModelDownload(this)
        val actions = MainActions(
            allowMic = { askMic.launch(Manifest.permission.RECORD_AUDIO) },
            download = {
                failure = null
                download.start()
                main.removeCallbacks(poll)
                main.post(poll)
            },
            cancelDownload = {
                download.cancel()
                refresh()
            },
            enableKeyboard = { Setup.openKeyboardSettings(this) },
            startWith = {
                Settings(this).startWith = it
                refresh()
            },
            openAccessibility = { Setup.openAccessibilitySettings(this) },
            openAppInfo = { Setup.openAppInfo(this) },
            getHeliBoard = { Setup.openHeliBoard(this) },
            copy = ::copy,
            share = { Setup.share(this, it) },
            delete = {
                AppFiles(this).transcripts.remove(it)
                refresh()
            },
            clear = {
                AppFiles(this).transcripts.clear()
                refresh()
            },
            addWord = { word ->
                val clean = word.trim()
                if (clean.isNotEmpty() && clean !in state.words) saveWords(state.words + clean)
            },
            removeWord = { word -> saveWords(state.words - word) },
            voiceNotes = { on ->
                Settings(this).voiceNotes = on
                refresh()
            },
            allowNotifications = {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
                } else {
                    Setup.openAppInfo(this)
                }
            },
            openNotificationAccess = { Setup.openNotificationAccess(this) },
            openAllFiles = { Setup.openAllFiles(this) },
        )
        setContent { DictateTheme { MainScreen(state, actions) } }
        // The download's size from the server, for the button (HEAD requests, off the main thread).
        Thread {
            val bytes = runCatching { download.size() }.getOrNull()
            main.post {
                modelBytes = bytes
                refresh()
            }
        }.start()
    }

    override fun onResume() {
        super.onResume()
        main.post(poll)
    }

    override fun onPause() {
        main.removeCallbacks(poll)
        super.onPause()
    }

    /** Back from the keyboard picker (a dialog: no onResume). */
    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) refresh()
    }

    private fun refresh() {
        val model = when (val status = download.status()) {
            ModelDownload.Status.Complete -> ModelState.Ready
            is ModelDownload.Status.Running -> ModelState.Downloading(status.done, status.total)
            is ModelDownload.Status.Failed -> {
                failure = status.reason
                ModelState.Failed(status.reason)
            }
            ModelDownload.Status.Idle -> failure?.let { ModelState.Failed(it) }
                ?: ModelState.Missing(modelBytes, download.metered())
        }
        state = MainState(
            micGranted = Setup.micGranted(this),
            model = model,
            startWith = Settings(this).startWith,
            buttonOn = Setup.buttonServiceOn(this),
            keyboardEnabled = Setup.keyboardEnabled(this),
            micKeyOpens = Setup.micKeyOpens(this),
            transcripts = AppFiles(this).transcripts.all(),
            words = words(),
            timing = Engine.lastTiming?.let { it.audioSeconds to it.millis / 1000f },
            voiceNotes = VoiceNotesState(
                on = Settings(this).voiceNotes,
                notifications = Setup.notificationsAllowed(this),
                listener = Setup.notificationAccess(this),
                files = Setup.allFiles(),
            ),
        )
    }

    private fun words(): List<String> =
        runCatching { Native.dictionary(AppFiles(this).data.path).toList() }.getOrDefault(emptyList())

    private fun saveWords(words: List<String>) {
        runCatching { Native.setDictionary(AppFiles(this).data.path, words.toTypedArray()) }
            .onFailure { Toast.makeText(this, Engine.describe(it), Toast.LENGTH_LONG).show() }
        Engine.dictionaryChanged()
        refresh()
    }

    private fun copy(text: String) {
        Setup.copy(this, text)
        // Android 13 and later confirm a copy themselves.
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            Toast.makeText(this, R.string.copied, Toast.LENGTH_SHORT).show()
        }
    }
}
