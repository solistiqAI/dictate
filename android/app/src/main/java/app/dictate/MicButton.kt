package app.dictate

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView

/**
 * The microphone button of the keyboard, the floating button and the listening dialog, in the
 * app icon's look (assets/app-icon.svg): a dark rounded tile with the five white level bars. While
 * recording the rim turns red and the bars move with the voice; while transcribing they dim.
 */
@SuppressLint("ViewConstructor")
class MicButton(context: Context, private val sizeDp: Int) : FrameLayout(context) {
    enum class Mode { IDLE, RECORDING, BUSY, SETUP }

    private val ring = View(context)
    val icon = ImageView(context)
    private val bars = LevelBars()
    private val tile = GradientDrawable(GradientDrawable.Orientation.TL_BR, intArrayOf(TILE_TOP, TILE_MIDDLE, TILE_BOTTOM))
    private val glow = GradientDrawable()

    init {
        val radius = dp(sizeDp) * CORNER
        tile.cornerRadius = radius
        glow.cornerRadius = radius * 1.25f
        ring.background = glow
        addView(ring, LayoutParams(dp(sizeDp + sizeDp / 4), dp(sizeDp + sizeDp / 4), Gravity.CENTER))
        icon.background = tile
        icon.setImageDrawable(bars)
        icon.scaleType = ImageView.ScaleType.FIT_XY
        icon.contentDescription = context.getString(R.string.keyboard_mic)
        icon.isClickable = true
        icon.elevation = dp(4).toFloat()
        addView(icon, LayoutParams(dp(sizeDp), dp(sizeDp), Gravity.CENTER))
        show(Mode.IDLE)
    }

    fun show(mode: Mode) {
        val recording = mode == Mode.RECORDING
        tile.setStroke(if (recording) dp(2) else maxOf(1, dp(1) / 2), if (recording) RECORDING else RIM)
        glow.setColor(if (recording) RECORDING and 0x00FFFFFF or 0x40000000 else 0)
        bars.alpha = if (mode == Mode.BUSY) 110 else 255
        bars.moving = recording
        icon.alpha = if (mode == Mode.SETUP) 0.6f else 1f
        if (!recording) level(0f)
    }

    /** The bars and the red glow follow the voice, 0 to 1. */
    fun level(value: Float) {
        val v = value.coerceIn(0f, 1f)
        val scale = 0.8f + 0.25f * v
        ring.scaleX = scale
        ring.scaleY = scale
        bars.level = v
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    /**
     * The app icon's five bars (heights 150, 300, 460, 260, 130 of 1024, 56 wide, 90 apart),
     * centred. Still: the icon's shape; [moving]: each bar between a third and its full height with
     * the voice, the middle ones most.
     */
    private class LevelBars : Drawable() {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFFFFFFF.toInt() }
        private val rect = RectF()
        var moving = false
            set(value) {
                field = value
                invalidateSelf()
            }
        var level = 0f
            set(value) {
                field = value
                invalidateSelf()
            }

        override fun draw(canvas: Canvas) {
            val box = bounds
            // The icon draws its tile 824 wide in 1024: the bars keep that proportion to the tile.
            val unit = box.width() / 824f
            val cx = box.exactCenterX()
            val cy = box.exactCenterY()
            for (i in HEIGHTS.indices) {
                val full = HEIGHTS[i] * unit
                val height = if (moving) {
                    val swing = (0.35f + 0.65f * level * WEIGHTS[i]).coerceIn(0.3f, 1f)
                    maxOf(WIDTH * unit, full * swing)
                } else {
                    full
                }
                val x = cx + (i - 2) * GAP * unit
                rect.set(x - WIDTH * unit / 2, cy - height / 2, x + WIDTH * unit / 2, cy + height / 2)
                canvas.drawRoundRect(rect, WIDTH * unit / 2, WIDTH * unit / 2, paint)
            }
        }

        override fun setAlpha(alpha: Int) {
            paint.alpha = alpha
            invalidateSelf()
        }

        override fun getAlpha(): Int = paint.alpha

        override fun setColorFilter(colorFilter: ColorFilter?) {
            paint.colorFilter = colorFilter
        }

        @Deprecated("Deprecated in Java")
        override fun getOpacity(): Int = PixelFormat.TRANSLUCENT

        companion object {
            val HEIGHTS = floatArrayOf(150f, 300f, 460f, 260f, 130f)
            val WEIGHTS = floatArrayOf(0.8f, 1.2f, 1.5f, 1.2f, 0.8f)
            const val WIDTH = 56f
            const val GAP = 90f
        }
    }

    companion object {
        /** The app icon's corner, relative to its size. */
        private const val CORNER = 0.3f
        const val TILE_TOP = 0xFF353A3D.toInt()
        const val TILE_MIDDLE = 0xFF1C2124.toInt()
        const val TILE_BOTTOM = 0xFF090D0F.toInt()
        const val RIM = 0xFF5A6064.toInt()
        const val RECORDING = 0xFFD7372F.toInt()
    }
}
