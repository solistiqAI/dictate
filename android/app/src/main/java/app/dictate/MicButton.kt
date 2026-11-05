package app.dictate

import android.annotation.SuppressLint
import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView

/**
 * The round microphone button of the keyboard, the floating button and the listening dialog: blue
 * when ready, red with a ring that grows with the voice while recording, grey while transcribing.
 */
@SuppressLint("ViewConstructor")
class MicButton(context: Context, private val sizeDp: Int) : FrameLayout(context) {
    enum class Mode { IDLE, RECORDING, BUSY, SETUP }

    private val ring = View(context)
    val icon = ImageView(context)

    init {
        ring.background = GradientDrawable().apply { shape = GradientDrawable.OVAL }
        addView(ring, LayoutParams(dp(sizeDp + sizeDp / 4), dp(sizeDp + sizeDp / 4), Gravity.CENTER))
        icon.background = GradientDrawable().apply { shape = GradientDrawable.OVAL }
        icon.scaleType = ImageView.ScaleType.CENTER
        icon.imageTintList = ColorStateList.valueOf(Color.WHITE)
        icon.contentDescription = context.getString(R.string.keyboard_mic)
        icon.isClickable = true
        addView(icon, LayoutParams(dp(sizeDp), dp(sizeDp), Gravity.CENTER))
        show(Mode.IDLE)
    }

    fun show(mode: Mode) {
        val color = when (mode) {
            Mode.IDLE, Mode.SETUP -> ACCENT
            Mode.RECORDING -> RECORDING
            Mode.BUSY -> BUSY
        }
        (icon.background as GradientDrawable).setColor(color)
        icon.setImageResource(if (mode == Mode.RECORDING) R.drawable.ic_stop else R.drawable.ic_mic)
        icon.alpha = if (mode == Mode.SETUP) 0.6f else 1f
        (ring.background as GradientDrawable).setColor(color and 0x00FFFFFF or 0x40000000)
        if (mode != Mode.RECORDING) level(0f)
    }

    /** The ring grows with the voice, 0 to 1. */
    fun level(value: Float) {
        val scale = 0.8f + 0.25f * value.coerceIn(0f, 1f)
        ring.scaleX = scale
        ring.scaleY = scale
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    companion object {
        const val ACCENT = 0xFF2F6BDB.toInt()
        const val RECORDING = 0xFFD7372F.toInt()
        const val BUSY = 0xFF8A9298.toInt()
    }
}
