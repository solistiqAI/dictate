package app.dictate

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.Gravity
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.TextView

/** When and how much of the live text: the pure part, tested on the JVM. */
object Live {
    /** The first pass, once there is something to hear. */
    const val FIRST_MILLIS = 700L

    /** Between passes at least this long, and twice as long as the last pass took. */
    const val INTERVAL_MILLIS = 600L

    /** Shorter than this, a pass hears nothing yet. */
    const val MIN_SAMPLES = Recorder.RATE / 2

    fun next(tookMillis: Long): Long = maxOf(INTERVAL_MILLIS, tookMillis * 2)

    /** The newest words of [text], at most [max] characters, cut at a word with "…" in front. */
    fun tail(text: String, max: Int = 160): String {
        val trimmed = text.trim()
        if (trimmed.length <= max) return trimmed
        val cut = trimmed.substring(trimmed.length - max)
        val space = cut.indexOf(' ')
        val words = if (space in 0 until cut.length - 1) cut.substring(space + 1) else cut
        return "…$words"
    }
}

/**
 * The text so far while recording, as on the desktop: the recording so far goes through the model
 * again and again (a phone CPU does 10 s of speech in about 0.4 s), one pass at a time, and [show]
 * gets each text. The final text still comes from the pass over the whole recording at the end.
 */
class LiveText(
    private val context: Context,
    private val listening: Listening,
    private val show: (String) -> Unit,
) {
    private val main = Handler(Looper.getMainLooper())
    private var running = false
    private val tick = Runnable { pass() }

    fun start() {
        running = true
        main.postDelayed(tick, Live.FIRST_MILLIS)
    }

    fun stop() {
        running = false
        main.removeCallbacks(tick)
    }

    private fun pass() {
        if (!running || !listening.active) return
        val samples = listening.snapshot()
        if (samples.size < Live.MIN_SAMPLES) {
            main.postDelayed(tick, Live.INTERVAL_MILLIS)
            return
        }
        val started = SystemClock.uptimeMillis()
        Engine.transcribe(context, samples, Recorder.RATE, live = true) { result ->
            if (!running) return@transcribe
            result.onSuccess { if (it.isNotBlank()) show(it) }
            main.postDelayed(tick, Live.next(SystemClock.uptimeMillis() - started))
        }
    }
}

/**
 * The live text above the floating button, across the screen: dark, three lines at most, the
 * newest words. It takes no touches, so the app below stays usable.
 */
@SuppressLint("ViewConstructor")
class LiveBubble(private val context: Context, private val windows: WindowManager) {
    private var view: TextView? = null
    private var params: WindowManager.LayoutParams? = null

    /** [buttonTop]: the floating button's top on [screen]. */
    fun show(text: String, buttonTop: Int, screen: Rect) {
        val density = context.resources.displayMetrics.density
        val margin = (12 * density).toInt()
        val gap = (8 * density).toInt()
        val shown = view ?: TextView(context).apply {
            setTextColor(Color.WHITE)
            textSize = 16f
            maxLines = 3
            val padX = (14 * density).toInt()
            val padY = (10 * density).toInt()
            setPadding(padX, padY, padX, padY)
            background = GradientDrawable().apply {
                cornerRadius = 18 * density
                setColor(BACKGROUND)
            }
        }
        shown.text = Live.tail(text)
        val layout = params ?: WindowManager.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT,
        ).apply { gravity = Gravity.BOTTOM or Gravity.START }
        layout.width = screen.width() - 2 * margin
        layout.x = margin
        layout.y = (screen.bottom - buttonTop + gap).coerceAtLeast(0)
        if (view == null) {
            windows.addView(shown, layout)
            view = shown
            params = layout
        } else {
            windows.updateViewLayout(shown, layout)
        }
    }

    fun hide() {
        view?.let { runCatching { windows.removeView(it) } }
        view = null
        params = null
    }

    private companion object {
        const val BACKGROUND = 0xE6202124.toInt()
    }
}
