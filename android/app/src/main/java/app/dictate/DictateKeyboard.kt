package app.dictate

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.inputmethodservice.InputMethodService
import android.os.SystemClock
import android.text.InputType
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager

/**
 * The voice keyboard. Opened from another keyboard (HeliBoard's mic key hands off to the voice
 * input method; Gboard's switch-keyboard key; the navigation bar's keyboard icon) it starts
 * listening at once, stops when you stop talking (or tap), puts the text in and switches back to
 * the keyboard you came from. Hold the button to talk again, or tap, talk, tap. Recording, the
 * model and the text all stay on the phone.
 */
class DictateKeyboard : InputMethodService(), KeyboardView.Listener {
    private var view: KeyboardView? = null
    private val gesture = MicGesture()
    private var listening: Listening? = null
    private var live: LiveText? = null

    /** The character before the cursor when recording started: how the text joins on. */
    private var before: CharSequence? = null

    /** Live text sits in the field as composing (underlined) text until the final one replaces it. */
    private var composing = false
    private var busy = false
    private var message: String? = null

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

    /** Shown: someone switched to the voice keyboard to talk, so listen right away. */
    override fun onWindowShown() {
        super.onWindowShown()
        if (listening == null && !busy && setupProblem() == null) {
            if (startListening(handsFree = true)) gesture.startedElsewhere()
        }
    }

    override fun onWindowHidden() {
        stopListening(keep = false)
        super.onWindowHidden()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        stopListening(keep = false)
        super.onFinishInputView(finishingInput)
    }

    /** Never the full-screen text box in landscape: the keyboard is small. */
    override fun onEvaluateFullscreenMode(): Boolean = false

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
            problem != null -> view.show(MicButton.Mode.SETUP, problem)
            listening != null -> view.show(MicButton.Mode.RECORDING, getString(R.string.keyboard_listening))
            busy -> view.show(MicButton.Mode.BUSY, getString(R.string.keyboard_transcribing))
            Engine.state == Engine.State.LOADING ->
                view.show(MicButton.Mode.IDLE, getString(R.string.keyboard_loading))
            else -> view.show(MicButton.Mode.IDLE, message ?: getString(R.string.keyboard_hint))
        }
    }

    override fun onMicDown() {
        if (busy) return
        if (setupProblem() != null) {
            openApp()
            return
        }
        when (gesture.down(SystemClock.uptimeMillis())) {
            MicGesture.Action.START -> if (!startListening(handsFree = false)) gesture.reset()
            MicGesture.Action.STOP -> stopListening(keep = true)
            MicGesture.Action.NONE -> {}
        }
    }

    override fun onMicUp() {
        if (gesture.up(SystemClock.uptimeMillis()) == MicGesture.Action.STOP) stopListening(keep = true)
    }

    private fun startListening(handsFree: Boolean): Boolean {
        val listen = Listening(
            this,
            handsFree,
            object : Listening.Events {
                override fun level(value: Float) {
                    view?.level(value)
                }

                // Hands-free, the end of speech stops it; the button's next press then starts anew.
                override fun finished(samples: FloatArray) {
                    live?.stop()
                    live = null
                    listening = null
                    gesture.reset()
                    transcribe(samples)
                }
            },
        )
        if (!listen.start()) {
            message = getString(R.string.keyboard_mic_failed)
            refresh()
            return false
        }
        message = null
        listening = listen
        before = currentInputConnection?.getTextBeforeCursor(1, 0)
        live = LiveText(this, listen) { text -> showLive(text) }.also { it.start() }
        refresh()
        return true
    }

    /** The words so far, in the field as composing text (not in password fields). */
    private fun showLive(text: String) {
        if (isPassword(currentInputEditorInfo)) return
        val connection = currentInputConnection ?: return
        connection.setComposingText(Typing.join(before, text), 1)
        composing = true
    }

    /** Takes the live text out of the field again. */
    private fun clearLive() {
        if (!composing) return
        composing = false
        currentInputConnection?.commitText("", 1)
    }

    /** [keep]: transcribe what was recorded; else drop it (the keyboard went away). */
    private fun stopListening(keep: Boolean) {
        val listen = listening ?: return
        if (keep) {
            listen.finish() // → finished() → transcribe
        } else {
            live?.stop()
            live = null
            listening = null
            listen.cancel()
            clearLive()
            gesture.reset()
            refresh()
        }
    }

    private fun transcribe(samples: FloatArray) {
        val keep = !isPassword(currentInputEditorInfo)
        busy = true
        refresh()
        Engine.transcribe(this, samples, Recorder.RATE) { result ->
            busy = false
            result.onSuccess { text ->
                if (text.isEmpty()) {
                    clearLive()
                    message = getString(R.string.keyboard_nothing)
                } else {
                    insert(text)
                    if (keep) {
                        AppFiles(this).transcripts.add(
                            Transcript(System.currentTimeMillis(), Transcript.Source.KEYBOARD, text),
                        )
                    }
                    // Done: back to the keyboard the user came from (nothing happens without one).
                    switchToPreviousInputMethod()
                }
            }.onFailure {
                clearLive()
                message = Engine.describe(it)
            }
            refresh()
        }
    }

    /** Replaces the live text (if any) with the final one. */
    private fun insert(text: String) {
        val connection = currentInputConnection ?: return
        val joinTo = if (composing) before else connection.getTextBeforeCursor(1, 0)
        composing = false
        connection.commitText(Typing.join(joinTo, text), 1)
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
        stopListening(keep = false)
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
