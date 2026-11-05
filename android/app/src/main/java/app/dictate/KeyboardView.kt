package app.dictate

import android.annotation.SuppressLint
import android.content.Context
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.os.Handler
import android.os.Looper
import android.util.TypedValue
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView

/**
 * The voice keyboard's view: one big microphone button, and next to it the keys needed between
 * dictations (next keyboard, backspace, space, enter). Plain views: an input method's window has
 * none of the lifecycle Compose expects.
 */
@SuppressLint("ViewConstructor")
class KeyboardView(context: Context, private val listener: Listener) : LinearLayout(context) {
    interface Listener {
        fun onMicDown()
        fun onMicUp()
        fun onBackspace()
        fun onSpace()
        fun onEnter()
        fun onSwitch()
        fun onSwitchLong()
    }

    private val dark = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) ==
        Configuration.UI_MODE_NIGHT_YES
    private val background = if (dark) 0xFF1B1F22.toInt() else 0xFFE8ECEF.toInt()
    private val keyColor = if (dark) 0xFF33393D.toInt() else Color.WHITE
    private val ink = if (dark) 0xFFE6E9EB.toInt() else 0xFF1B1F22.toInt()
    private val subtle = if (dark) 0xFFA4ABB0.toInt() else 0xFF5B6368.toInt()

    private val status = TextView(context)
    private val mic = MicButton(context, 72)
    private val repeat = Handler(Looper.getMainLooper())

    init {
        orientation = VERTICAL
        setBackgroundColor(background)
        setPadding(dp(8), dp(6), dp(8), dp(8))

        status.setTextColor(subtle)
        status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
        status.gravity = Gravity.CENTER
        status.maxLines = 2
        addView(status, LayoutParams(LayoutParams.MATCH_PARENT, dp(36)))

        val middle = LinearLayout(context).apply { gravity = Gravity.CENTER_VERTICAL }
        val switch = key(R.drawable.ic_keyboard, R.string.keyboard_switch)
        tap(switch) { listener.onSwitch() }
        switch.setOnLongClickListener {
            listener.onSwitchLong()
            true
        }
        middle.addView(switch, keyParams(52, 1f))
        middle.addView(micButton(), LayoutParams(0, dp(112), 1.6f))
        val backspace = key(R.drawable.ic_backspace, R.string.keyboard_backspace)
        repeating(backspace) { listener.onBackspace() }
        middle.addView(backspace, keyParams(52, 1f))
        addView(middle, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))

        val bottom = LinearLayout(context)
        val space = key(null, R.string.keyboard_space)
        tap(space) { listener.onSpace() }
        bottom.addView(space, keyParams(48, 3f))
        val enter = key(R.drawable.ic_enter, R.string.keyboard_enter)
        tap(enter) { listener.onEnter() }
        bottom.addView(enter, keyParams(48, 1f))
        addView(bottom, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT).apply { topMargin = dp(6) })

        show(MicButton.Mode.IDLE, context.getString(R.string.keyboard_hint))
    }

    /** The button's look and the line above it. */
    fun show(mode: MicButton.Mode, message: String) {
        status.text = message
        mic.show(mode)
    }

    /** The ring around the button grows with the voice, 0 to 1. */
    fun level(value: Float) = mic.level(value)

    @SuppressLint("ClickableViewAccessibility")
    private fun micButton(): View {
        mic.icon.setOnTouchListener { view, event ->
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    listener.onMicDown()
                }
                MotionEvent.ACTION_UP -> {
                    view.performClick()
                    listener.onMicUp()
                }
                MotionEvent.ACTION_CANCEL -> listener.onMicUp()
            }
            true
        }
        return mic
    }

    private fun key(icon: Int?, label: Int): View {
        val description = context.getString(label)
        val view: View = if (icon != null) {
            ImageView(context).apply {
                setImageResource(icon)
                scaleType = ImageView.ScaleType.CENTER
                imageTintList = ColorStateList.valueOf(ink)
                contentDescription = description
            }
        } else {
            TextView(context).apply {
                text = description
                gravity = Gravity.CENTER
                setTextColor(subtle)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            }
        }
        view.background = GradientDrawable().apply {
            cornerRadius = dp(8).toFloat()
            setColor(keyColor)
        }
        view.isClickable = true
        return view
    }

    private fun keyParams(height: Int, weight: Float) = LayoutParams(0, dp(height), weight).apply {
        setMargins(dp(3), dp(3), dp(3), dp(3))
    }

    private fun tap(view: View, action: () -> Unit) {
        view.setOnClickListener {
            it.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
            action()
        }
    }

    /** Fires on press and then repeats while held (backspace). */
    @SuppressLint("ClickableViewAccessibility")
    private fun repeating(view: View, action: () -> Unit) {
        val again = object : Runnable {
            override fun run() {
                action()
                repeat.postDelayed(this, 60)
            }
        }
        view.setOnTouchListener { v, event ->
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    v.isPressed = true
                    v.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    action()
                    repeat.postDelayed(again, 400)
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    v.isPressed = false
                    repeat.removeCallbacks(again)
                    if (event.actionMasked == MotionEvent.ACTION_UP) v.performClick()
                }
            }
            true
        }
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()
}
