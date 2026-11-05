package app.dictate

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.text.InputType
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager

/**
 * The voice keyboard: hold the microphone to talk (or tap, talk, tap), the text goes in at the
 * cursor. Recording, the model and the text all stay on the phone.
 */
class DictateKeyboard : InputMethodService(), KeyboardView.Listener {
    private var view: KeyboardView? = null
    private val gesture = MicGesture()
    private var recorder: Recorder? = null
    private var busy = false
    private var message: String? = null
    private val main = Handler(Looper.getMainLooper())

    private val levelTick = object : Runnable {
        override fun run() {
            val recording = recorder ?: return
            view?.level(recording.level)
            main.postDelayed(this, 50)
        }
    }

    override fun onCreateInputView(): View = KeyboardView(this, this).also {
        view = it
        refresh()
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        message = null
        refresh()
        // Load the model while the user finds the button (seconds, once per process).
        if (setupProblem() == null && Engine.state == Engine.State.IDLE) {
            Engine.prepare(this) { refresh() }
            refresh()
        }
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        if (recorder != null) {
            recorder?.stop()
            recorder = null
            main.removeCallbacks(levelTick)
        }
        gesture.reset()
        super.onFinishInputView(finishingInput)
    }

    /** What is missing before the keyboard can work, as the line to show. */
    private fun setupProblem(): String? = when {
        checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED ->
            getString(R.string.keyboard_needs_mic)
        !ModelDownload(this).installed() -> getString(R.string.keyboard_needs_model)
        else -> null
    }

    private fun refresh() {
        val view = view ?: return
        val problem = setupProblem()
        when {
            problem != null -> view.show(KeyboardView.Mode.SETUP, problem)
            recorder != null -> view.show(KeyboardView.Mode.RECORDING, getString(R.string.keyboard_listening))
            busy -> view.show(KeyboardView.Mode.BUSY, getString(R.string.keyboard_transcribing))
            Engine.state == Engine.State.LOADING ->
                view.show(KeyboardView.Mode.IDLE, getString(R.string.keyboard_loading))
            else -> view.show(KeyboardView.Mode.IDLE, message ?: getString(R.string.keyboard_hint))
        }
    }

    override fun onMicDown() {
        if (busy) return
        if (setupProblem() != null) {
            openApp()
            return
        }
        when (gesture.down(SystemClock.uptimeMillis())) {
            MicGesture.Action.START -> startRecording()
            MicGesture.Action.STOP -> stopAndTranscribe()
            MicGesture.Action.NONE -> {}
        }
    }

    override fun onMicUp() {
        if (gesture.up(SystemClock.uptimeMillis()) == MicGesture.Action.STOP) stopAndTranscribe()
    }

    private fun startRecording() {
        val recording = Recorder()
        if (!recording.start(this)) {
            gesture.reset()
            message = getString(R.string.keyboard_mic_failed)
            refresh()
            return
        }
        message = null
        recorder = recording
        main.post(levelTick)
        refresh()
    }

    private fun stopAndTranscribe() {
        val recording = recorder ?: return
        recorder = null
        main.removeCallbacks(levelTick)
        val samples = recording.stop()
        val keep = !isPassword(currentInputEditorInfo)
        busy = true
        refresh()
        Engine.transcribe(this, samples, Recorder.RATE) { result ->
            busy = false
            result.onSuccess { text ->
                if (text.isNotEmpty()) {
                    insert(text)
                    if (keep) {
                        AppFiles(this).transcripts.add(
                            Transcript(System.currentTimeMillis(), Transcript.Source.KEYBOARD, text),
                        )
                    }
                }
            }.onFailure { message = Engine.describe(it) }
            refresh()
        }
    }

    private fun insert(text: String) {
        val connection = currentInputConnection ?: return
        connection.commitText(Typing.join(connection.getTextBeforeCursor(1, 0), text), 1)
    }

    override fun onBackspace() {
        sendDownUpKeyEvents(KeyEvent.KEYCODE_DEL)
    }

    override fun onSpace() {
        currentInputConnection?.commitText(" ", 1)
    }

    override fun onEnter() {
        val info = currentInputEditorInfo
        val action = info?.imeOptions?.and(EditorInfo.IME_MASK_ACTION) ?: EditorInfo.IME_ACTION_NONE
        val noAction = info == null || info.imeOptions and EditorInfo.IME_FLAG_NO_ENTER_ACTION != 0 ||
            action == EditorInfo.IME_ACTION_NONE || action == EditorInfo.IME_ACTION_UNSPECIFIED
        if (noAction) {
            sendDownUpKeyEvents(KeyEvent.KEYCODE_ENTER)
        } else {
            currentInputConnection?.performEditorAction(action)
        }
    }

    override fun onSwitch() {
        if (!switchToPreviousInputMethod()) onSwitchLong()
    }

    override fun onSwitchLong() {
        getSystemService(InputMethodManager::class.java)?.showInputMethodPicker()
    }

    private fun openApp() {
        startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    private fun isPassword(info: EditorInfo?): Boolean {
        val type = info?.inputType ?: return false
        val variation = type and InputType.TYPE_MASK_VARIATION
        return when (type and InputType.TYPE_MASK_CLASS) {
            InputType.TYPE_CLASS_TEXT -> variation == InputType.TYPE_TEXT_VARIATION_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD
            InputType.TYPE_CLASS_NUMBER -> variation == InputType.TYPE_NUMBER_VARIATION_PASSWORD
            else -> false
        }
    }
}
