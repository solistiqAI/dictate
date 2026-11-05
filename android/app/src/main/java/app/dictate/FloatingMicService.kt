package app.dictate

import android.accessibilityservice.AccessibilityService
import android.annotation.SuppressLint
import android.content.Intent
import android.graphics.PixelFormat
import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.MotionEvent
import android.view.ViewConfiguration
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import android.widget.Toast
import kotlin.math.abs

/**
 * The floating mic button, for any keyboard (Gboard's own mic key always opens Google's voice
 * typing): a small round button just above the keyboard while a text field has focus. Tap: record;
 * tap again: stop, transcribe, and the text goes into the field. Drag it anywhere; it keeps its
 * place (relative to the keyboard's top). Needs nothing but this accessibility service, which reads
 * which field has focus and writes into it; nothing is read or kept otherwise.
 */
class FloatingMicService : AccessibilityService() {
    private lateinit var windows: WindowManager
    private val main = Handler(Looper.getMainLooper())
    private var button: MicButton? = null
    private var params: WindowManager.LayoutParams? = null
    private var listening: Listening? = null
    private var busy = false
    private var target: AccessibilityNodeInfo? = null
    private var anchor = 0
    private val updateSoon = Runnable { update() }

    override fun onServiceConnected() {
        super.onServiceConnected()
        instance = this
        windows = getSystemService(WindowManager::class.java)
        update()
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        // Events come in bursts: look once they settle.
        main.removeCallbacks(updateSoon)
        main.postDelayed(updateSoon, 80)
    }

    override fun onInterrupt() {}

    override fun onDestroy() {
        instance = null
        listening?.cancel()
        listening = null
        MicService.stop()
        hide()
        super.onDestroy()
    }

    /** The notification's Stop. */
    fun stopFromNotification() {
        listening?.finish()
    }

    /** Show the button when a text field has focus (and while it records or transcribes). */
    private fun update() {
        val wanted = Settings(this).startWith.button && ModelDownload(this).installed()
        val working = listening != null || busy
        // Dictate's own keyboard has the big button already.
        val field = editableFocus()?.takeIf { !Setup.keyboardChosen(this) }
        if (!wanted || (field == null && !working)) {
            hide()
            return
        }
        show(keyboardTop())
    }

    private fun editableFocus(): AccessibilityNodeInfo? =
        findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            ?.takeIf { it.isEditable && !it.isPassword && it.packageName != packageName }

    /** The top of the keyboard on screen, or the bottom of the screen without one. */
    private fun keyboardTop(): Int {
        val bounds = Rect()
        for (window in windows()) {
            if (window.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD) {
                window.getBoundsInScreen(bounds)
                return bounds.top
            }
        }
        return screen().bottom
    }

    private fun windows(): List<AccessibilityWindowInfo> = runCatching { getWindows() }.getOrDefault(emptyList())

    private fun screen(): Rect =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            windows.currentWindowMetrics.bounds
        } else {
            val metrics = resources.displayMetrics
            Rect(0, 0, metrics.widthPixels, metrics.heightPixels)
        }

    private fun size(): Int = (64 * resources.displayMetrics.density).toInt()

    private fun show(top: Int) {
        anchor = top
        val settings = Settings(this)
        val screen = screen()
        val size = size()
        val margin = (12 * resources.displayMetrics.density).toInt()
        val x = settings.buttonX.takeIf { it >= 0 } ?: (screen.right - size - margin)
        val above = settings.buttonAbove.takeIf { it >= 0 } ?: margin
        val layout = params ?: WindowManager.LayoutParams(
            size,
            size,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT,
        ).apply { gravity = Gravity.TOP or Gravity.START }
        layout.x = x.coerceIn(0, maxOf(0, screen.right - size))
        layout.y = (top - above - size).coerceIn(0, maxOf(0, screen.bottom - size))
        val view = button
        if (view == null) {
            val created = MicButton(this, 52)
            created.icon.setOnTouchListener(Drag(layout))
            windows.addView(created, layout)
            button = created
            params = layout
            refresh()
        } else {
            windows.updateViewLayout(view, layout)
        }
    }

    private fun hide() {
        button?.let { runCatching { windows.removeView(it) } }
        button = null
        params = null
    }

    private fun refresh() {
        button?.show(
            when {
                listening != null -> MicButton.Mode.RECORDING
                busy -> MicButton.Mode.BUSY
                else -> MicButton.Mode.IDLE
            },
        )
    }

    /** Drag to move (remembered), tap to record or stop. */
    private inner class Drag(private val layout: WindowManager.LayoutParams) : android.view.View.OnTouchListener {
        private val slop = ViewConfiguration.get(this@FloatingMicService).scaledTouchSlop
        private var downX = 0f
        private var downY = 0f
        private var startX = 0
        private var startY = 0
        private var dragging = false

        @SuppressLint("ClickableViewAccessibility")
        override fun onTouch(view: android.view.View, event: MotionEvent): Boolean {
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    downX = event.rawX
                    downY = event.rawY
                    startX = layout.x
                    startY = layout.y
                    dragging = false
                }
                MotionEvent.ACTION_MOVE -> {
                    val dx = event.rawX - downX
                    val dy = event.rawY - downY
                    if (!dragging && (abs(dx) > slop || abs(dy) > slop)) dragging = true
                    if (dragging) {
                        layout.x = startX + dx.toInt()
                        layout.y = startY + dy.toInt()
                        button?.let { windows.updateViewLayout(it, layout) }
                    }
                }
                MotionEvent.ACTION_UP -> {
                    if (dragging) {
                        val settings = Settings(this@FloatingMicService)
                        settings.buttonX = layout.x
                        settings.buttonAbove = maxOf(0, anchor - layout.y - size())
                    } else {
                        view.performClick()
                        toggle()
                    }
                }
            }
            return true
        }
    }

    private fun toggle() {
        if (busy) return
        listening?.let {
            it.finish()
            return
        }
        if (!Setup.micGranted(this)) {
            startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            return
        }
        target = editableFocus()
        val listen = Listening(
            this,
            handsFree = false,
            object : Listening.Events {
                override fun level(value: Float) {
                    button?.level(value)
                }

                override fun finished(samples: FloatArray) {
                    listening = null
                    transcribe(samples)
                }
            },
        )
        MicService.start(this)
        if (!listen.start()) {
            MicService.stop()
            Toast.makeText(this, R.string.keyboard_mic_failed, Toast.LENGTH_SHORT).show()
            return
        }
        listening = listen
        refresh()
    }

    private fun transcribe(samples: FloatArray) {
        busy = true
        refresh()
        Engine.transcribe(this, samples, Recorder.RATE) { result ->
            busy = false
            MicService.stop()
            result.onSuccess { text ->
                if (text.isNotBlank()) {
                    insert(text)
                    AppFiles(this).transcripts.add(Transcript(System.currentTimeMillis(), Transcript.Source.KEYBOARD, text))
                } else {
                    Toast.makeText(this, R.string.keyboard_nothing, Toast.LENGTH_SHORT).show()
                }
            }.onFailure { Toast.makeText(this, Engine.describe(it), Toast.LENGTH_LONG).show() }
            target = null
            refresh()
            update()
        }
    }

    /**
     * Into the field that had focus when recording started (else the focused one): set its text
     * with ours at the selection where it lets us (no clipboard involved), else paste. An
     * accessibility service may write the clipboard but not read it, so a paste leaves the text
     * there instead of restoring what was on it.
     */
    private fun insert(text: String) {
        val node = target?.takeIf { it.refresh() && it.isEditable } ?: editableFocus()
        if (node == null) {
            Setup.copy(this, text)
            Toast.makeText(this, R.string.button_copied, Toast.LENGTH_SHORT).show()
            return
        }
        val current = if (node.isShowingHintText) "" else node.text?.toString().orEmpty()
        val canSet = node.actionList.any { it.id == AccessibilityNodeInfo.ACTION_SET_TEXT }
        if (canSet) {
            val edit = Insertion.of(current, node.textSelectionStart, node.textSelectionEnd, text)
            val args = Bundle().apply {
                putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, edit.text)
            }
            if (node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)) {
                node.performAction(
                    AccessibilityNodeInfo.ACTION_SET_SELECTION,
                    Bundle().apply {
                        putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, edit.cursor)
                        putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, edit.cursor)
                    },
                )
                return
            }
        }
        val before = if (node.textSelectionStart > 0) current.substring(0, minOf(node.textSelectionStart, current.length)) else current
        Setup.copy(this, Typing.join(before, text))
        if (!node.performAction(AccessibilityNodeInfo.ACTION_PASTE)) {
            Toast.makeText(this, R.string.button_copied, Toast.LENGTH_SHORT).show()
        }
    }

    companion object {
        /** The running service, for the notification's Stop. */
        var instance: FloatingMicService? = null
            private set
    }
}
