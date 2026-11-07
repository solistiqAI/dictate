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
import android.text.format.DateUtils
import android.view.Gravity
import android.view.MotionEvent
import android.view.ViewConfiguration
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Toast
import kotlin.math.abs

/**
 * The floating mic button, for any keyboard (Gboard's own mic key always opens Google's voice
 * typing): a small round button while a text field has focus. Tap: record; tap again: stop,
 * transcribe, and the text goes into the field. Drag it anywhere, over the keyboard too; it keeps
 * that place on the screen and does not follow the keyboard. Needs nothing but this accessibility service, which reads
 * which field has focus and writes into it; nothing is read or kept otherwise.
 */
class FloatingMicService : AccessibilityService() {
    private lateinit var windows: WindowManager
    private val main = Handler(Looper.getMainLooper())
    private var button: MicButton? = null
    private var params: WindowManager.LayoutParams? = null
    private var listening: Listening? = null
    private var live: LiveText? = null
    private lateinit var bubble: LiveBubble
    private lateinit var strip: ChatStrip

    /** Per chat: the newest voice message when the user folded the strip into its chip. */
    private val closed = HashMap<String, Long>()

    /** Tapped, waiting for the microphone service before recording. */
    private var starting = false
    private var busy = false
    private var target: AccessibilityNodeInfo? = null
    private val updateSoon = Runnable { update() }

    /** A finger moves the button. */
    private var dragging = false

    override fun onServiceConnected() {
        super.onServiceConnected()
        instance = this
        windows = getSystemService(WindowManager::class.java)
        bubble = LiveBubble(this, windows)
        strip = ChatStrip(this, windows)
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
        live?.stop()
        live = null
        listening?.cancel()
        listening = null
        starting = false
        MicService.stop()
        hide()
        strip.hide()
        super.onDestroy()
    }

    /** The notification's Stop. */
    fun stopFromNotification() {
        listening?.finish()
    }

    /** A voice message was transcribed: an open chat's strip shows it. */
    fun voiceNotesChanged() {
        update()
    }

    /** Show the button when a text field has focus (and while it records or transcribes). */
    private fun update() {
        updateStrip()
        val wanted = Settings(this).startWith.button && ModelDownload(this).installed()
        val working = listening != null || busy || starting
        // Dictate's own keyboard has the big button already.
        val field = editableFocus()?.takeIf { !Setup.keyboardChosen(this) }
        // While it records (also when a keyboard's mic key started it), the button stays: it stops.
        if (!working && (!wanted || field == null)) {
            hide()
            return
        }
        // While a finger moves the button, it stays where the finger is.
        if (dragging) return
        show()
    }

    /**
     * Over an open WhatsApp chat, its transcribed voice messages of the last days: the chat's name
     * comes from WhatsApp's header (the only thing read there), the texts from [VoiceNoteLog].
     */
    private fun updateStrip() {
        val chat = if (Settings(this).voiceNotes) openChat() else null
        if (chat == null) {
            strip.hide()
            return
        }
        val since = System.currentTimeMillis() - STRIP_DAYS * DateUtils.DAY_IN_MILLIS
        val notes = AppFiles(this).voiceNotes.forChat(chat.name, since)
        val key = chat.name.lowercase()
        if (notes.isEmpty()) {
            strip.hide()
            return
        }
        // Folded until the chat's next voice message.
        val folded = (closed[key] ?: 0L) >= notes.first().time
        strip.show(
            notes,
            chat.bottom,
            chat.group,
            folded,
            onClose = {
                closed[key] = notes.first().time
                updateStrip()
            },
            onOpen = {
                closed.remove(key)
                updateStrip()
            },
        )
    }

    /** The WhatsApp chat in front, for the debug build's test voice message. */
    fun openChatName(): String? = openChat()?.name

    private class OpenChat(val name: String, val bottom: Int, val group: Boolean)

    /** The WhatsApp chat in front: its name and the bottom of its header, or null. */
    private fun openChat(): OpenChat? {
        val root = runCatching { rootInActiveWindow }.getOrNull() ?: return null
        val app = root.packageName?.toString() ?: return null
        if (app !in VoiceNotes.PACKAGES) return null
        val title = root.findAccessibilityNodeInfosByViewId("$app:id/conversation_contact_name").firstOrNull()
            ?: return null
        val name = title.text?.toString()?.takeIf { it.isNotBlank() } ?: return null
        val bounds = Rect()
        val header = root.findAccessibilityNodeInfosByViewId("$app:id/toolbar").firstOrNull()
        (header ?: title).getBoundsInScreen(bounds)
        // A group's header shows its members below the name.
        val group = root.findAccessibilityNodeInfosByViewId("$app:id/conversation_contact_status")
            .firstOrNull()?.text?.contains(",") == true
        return OpenChat(name, bounds.bottom, group)
    }

    private fun editableFocus(): AccessibilityNodeInfo? =
        findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            ?.takeIf { it.isEditable && !it.isPassword && it.packageName != packageName }

    private fun screen(): Rect =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            windows.currentWindowMetrics.bounds
        } else {
            val metrics = resources.displayMetrics
            Rect(0, 0, metrics.widthPixels, metrics.heightPixels)
        }

    private fun size(): Int = (64 * resources.displayMetrics.density).toInt()

    /**
     * Where the user put it, on the screen, and nowhere else: it does not follow the keyboard, so it
     * never moves while typing. First time: on the right, about where a keyboard's top is.
     */
    private fun show() {
        val settings = Settings(this)
        val screen = screen()
        val size = size()
        val margin = (12 * resources.displayMetrics.density).toInt()
        val x = settings.buttonX.takeIf { it >= 0 } ?: (screen.right - size - margin)
        val y = settings.buttonY ?: (screen.bottom * DEFAULT_Y).toInt()
        val layout = params ?: WindowManager.LayoutParams(
            size,
            size,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT,
        ).apply { gravity = Gravity.TOP or Gravity.START }
        val newX = x.coerceIn(0, maxOf(0, screen.right - size))
        val newY = y.coerceIn(0, maxOf(0, screen.bottom - size))
        val view = button
        // Every typed letter brings events: only touch the window when the place changed.
        if (view != null && newX == layout.x && newY == layout.y) return
        layout.x = newX
        layout.y = newY
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
        bubble.hide()
        button?.let { runCatching { windows.removeView(it) } }
        button = null
        params = null
    }

    private fun refresh() {
        button?.show(
            when {
                listening != null || starting -> MicButton.Mode.RECORDING
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
                        val screen = screen()
                        val size = size()
                        layout.x = (startX + dx.toInt()).coerceIn(0, maxOf(0, screen.right - size))
                        layout.y = (startY + dy.toInt()).coerceIn(0, maxOf(0, screen.bottom - size))
                        button?.let { windows.updateViewLayout(it, layout) }
                    }
                }
                MotionEvent.ACTION_UP -> {
                    if (dragging) {
                        dragging = false
                        val settings = Settings(this@FloatingMicService)
                        settings.buttonX = layout.x
                        settings.buttonY = layout.y
                        update()
                    } else {
                        view.performClick()
                        toggle()
                    }
                }
                MotionEvent.ACTION_CANCEL -> {
                    if (dragging) {
                        dragging = false
                        update()
                    }
                }
            }
            return true
        }
    }

    private fun toggle() {
        if (busy || starting) return
        listening?.let {
            it.finish()
            return
        }
        if (!Setup.micGranted(this)) {
            startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            return
        }
        record(editableFocus(), handsFree = false)
    }

    /**
     * A keyboard's mic key opened the voice keyboard, which hands over to here so the user's own
     * keyboard comes straight back and the live text shows above it, as with the button: start
     * listening for the focused field (hands-free: a pause ends it), or stop when it already
     * listens. False when it can't (no microphone permission, no text field).
     */
    fun fromKeyboard(): Boolean {
        if (busy || starting) return true
        listening?.let {
            it.finish()
            return true
        }
        if (!Setup.micGranted(this)) return false
        val field = editableFocus() ?: return false
        record(field, handsFree = true)
        return true
    }

    private fun record(field: AccessibilityNodeInfo?, handsFree: Boolean) {
        target = field
        val listen = Listening(
            this,
            handsFree = handsFree,
            object : Listening.Events {
                override fun level(value: Float) {
                    button?.level(value)
                }

                override fun finished(samples: FloatArray) {
                    val silenced = listening?.silenced == true
                    live?.stop()
                    live = null
                    listening = null
                    transcribe(samples, silenced)
                }
            },
        )
        // The button turns red at once; recording starts when the service runs (else silence).
        starting = true
        refresh()
        MicService.start(this) {
            if (!starting) return@start
            starting = false
            if (!listen.start()) {
                MicService.stop()
                Toast.makeText(this, R.string.keyboard_mic_failed, Toast.LENGTH_SHORT).show()
                refresh()
                update()
                return@start
            }
            listening = listen
            live = LiveText(this, listen) { text -> showLive(text) }.also { it.start() }
            refresh()
            update()
        }
    }

    private fun showLive(text: String) {
        val layout = params ?: return
        bubble.show(text, layout.y, screen())
    }

    private fun transcribe(samples: FloatArray, silenced: Boolean) {
        busy = true
        refresh()
        Engine.transcribe(this, samples, Recorder.RATE) { result ->
            busy = false
            bubble.hide()
            MicService.stop()
            result.onSuccess { text ->
                if (text.isNotBlank()) {
                    insert(text)
                    AppFiles(this).transcripts.add(Transcript(System.currentTimeMillis(), Transcript.Source.KEYBOARD, text))
                } else {
                    val why = if (silenced) R.string.mic_silenced else R.string.keyboard_nothing
                    Toast.makeText(this, why, Toast.LENGTH_LONG).show()
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
        val current = Insertion.fieldText(node.text, node.hintText, node.isShowingHintText)
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
        /** How far back the strip over a chat goes. */
        private const val STRIP_DAYS = 3L

        /** The button's first place, from the top, as a share of the screen: just above most keyboards. */
        private const val DEFAULT_Y = 0.5

        /** The running service, for the notification's Stop. */
        var instance: FloatingMicService? = null
            private set
    }
}
