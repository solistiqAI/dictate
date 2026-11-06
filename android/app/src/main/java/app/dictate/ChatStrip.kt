package app.dictate

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.text.TextUtils
import android.text.format.DateUtils
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast

/**
 * The strip over an open WhatsApp chat with its transcribed voice messages: one line with the
 * newest; a tap shows them all (newest first, each in full), a long press on one copies it. ✕
 * folds it into a small chip ("🎤 3") at the right, which opens it again; the chat's next voice
 * message opens it by itself.
 */
@SuppressLint("SetTextI18n")
class ChatStrip(private val context: Context, private val windows: WindowManager) {
    private var view: LinearLayout? = null
    private var shown: List<VoiceNote> = emptyList()
    private var expanded = false
    private var top = 0
    private var onClose: () -> Unit = {}
    private var onOpen: () -> Unit = {}
    private var folded = false
    private val density = context.resources.displayMetrics.density

    /**
     * [notes]: newest first; [top]: just below the chat's header, in screen pixels; [folded]: only
     * the chip.
     */
    fun show(notes: List<VoiceNote>, top: Int, group: Boolean, folded: Boolean, onClose: () -> Unit, onOpen: () -> Unit) {
        if (notes != shown) expanded = false
        this.onClose = onClose
        this.onOpen = onOpen
        val changed = notes != shown || this.top != top || view == null || this.folded != folded
        shown = notes
        this.top = top
        this.folded = folded
        if (changed) render(group)
    }

    fun hide() {
        view?.let { runCatching { windows.removeView(it) } }
        view = null
        shown = emptyList()
        expanded = false
    }

    private fun render(group: Boolean) {
        if (folded) {
            chip()
            return
        }
        val root = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                cornerRadius = 16 * density
                setColor(BACKGROUND)
            }
            setPadding(dp(14), dp(8), dp(6), dp(8))
        }
        val head = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        val newest = shown.first()
        val summary = text(
            if (expanded) {
                context.getString(R.string.strip_title, shown.size)
            } else {
                "🎤 ${label(newest, group)}  ${newest.text}"
            },
            bold = expanded,
        ).apply {
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
        }
        head.addView(summary, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        if (!expanded && shown.size > 1) head.addView(text(" +${shown.size - 1}", dim = true))
        head.addView(
            text("✕", dim = true).apply {
                setPadding(dp(12), dp(4), dp(10), dp(4))
                contentDescription = context.getString(R.string.strip_close)
                setOnClickListener { onClose() }
            },
        )
        root.addView(head)
        if (expanded) {
            val list = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
            for (note in shown) {
                list.addView(
                    text(label(note, group), dim = true).apply { setPadding(0, dp(10), 0, dp(2)) },
                )
                list.addView(
                    text(note.text).apply {
                        setTextIsSelectable(false)
                        setPadding(0, 0, dp(8), 0)
                        setOnLongClickListener {
                            Setup.copy(context, note.text)
                            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
                                Toast.makeText(context, R.string.copied, Toast.LENGTH_SHORT).show()
                            }
                            true
                        }
                    },
                )
            }
            val scroll = ScrollView(context).apply { addView(list) }
            val maxHeight = (context.resources.displayMetrics.heightPixels * 0.45f).toInt()
            root.addView(scroll, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
            scroll.post {
                if (scroll.height > maxHeight) {
                    scroll.layoutParams = scroll.layoutParams.apply { height = maxHeight }
                }
            }
        }
        root.setOnClickListener {
            expanded = !expanded
            render(group)
        }
        val margin = dp(8)
        val layout = WindowManager.LayoutParams(
            context.resources.displayMetrics.widthPixels - 2 * margin,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.START
            x = margin
            y = top + dp(4)
        }
        place(root, layout)
    }

    /** Folded: "🎤 3" at the right below the header; a tap opens the strip again. */
    private fun chip() {
        val root = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = GradientDrawable().apply {
                cornerRadius = 16 * density
                setColor(BACKGROUND)
            }
            setPadding(dp(12), dp(6), dp(12), dp(6))
            contentDescription = context.getString(R.string.strip_open)
            addView(text("🎤 ${shown.size}"))
            setOnClickListener { onOpen() }
        }
        val layout = WindowManager.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.END
            x = dp(12)
            y = top + dp(6)
        }
        place(root, layout)
    }

    private fun place(root: View, layout: WindowManager.LayoutParams) {
        view?.let { runCatching { windows.removeView(it) } }
        windows.addView(root, layout)
        view = root as? LinearLayout
    }

    /** "11:42", "Mo. 11:42"; in a group with the sender first. */
    private fun label(note: VoiceNote, group: Boolean): String {
        val flags = if (DateUtils.isToday(note.time)) {
            DateUtils.FORMAT_SHOW_TIME
        } else {
            DateUtils.FORMAT_SHOW_TIME or DateUtils.FORMAT_SHOW_WEEKDAY or DateUtils.FORMAT_ABBREV_WEEKDAY
        }
        val time = DateUtils.formatDateTime(context, note.time, flags)
        return if (group && note.sender.isNotBlank()) "${note.sender} · $time" else time
    }

    private fun text(value: String, dim: Boolean = false, bold: Boolean = false) = TextView(context).apply {
        text = value
        textSize = if (dim) 13f else 15f
        setTextColor(if (dim) DIM else Color.WHITE)
        if (bold) setTypeface(typeface, android.graphics.Typeface.BOLD)
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
    }

    private fun dp(value: Int): Int = (value * density).toInt()

    private companion object {
        const val BACKGROUND = 0xF0202124.toInt()
        const val DIM = 0xFFB0B5BA.toInt()
    }
}
