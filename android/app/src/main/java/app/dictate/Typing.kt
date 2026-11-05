package app.dictate

/** How dictated text joins what is already in the field. */
object Typing {
    private val noSpaceBefore = setOf('.', ',', '!', '?', ';', ':', ')', '\n')

    /**
     * [text] as it goes in after [before] (the text before the cursor; null when the field does
     * not say): a space between words, none at the start, after a space or a line break, or before
     * punctuation.
     */
    fun join(before: CharSequence?, text: String): String {
        if (text.isEmpty()) return text
        if (before.isNullOrEmpty()) return text
        val last = before[before.length - 1]
        if (last.isWhitespace() || last == '(') return text
        if (text[0] in noSpaceBefore || text[0].isWhitespace()) return text
        return " $text"
    }
}

/**
 * The microphone button: hold to talk (released after [holdMillis]: stop and transcribe), or tap
 * to start and tap again to stop.
 */
class MicGesture(private val holdMillis: Long = 400) {
    enum class Action { START, STOP, NONE }

    var recording = false
        private set
    private var pressedAt = 0L
    private var startedByThisPress = false

    fun down(now: Long): Action {
        pressedAt = now
        if (recording) {
            startedByThisPress = false
            recording = false
            return Action.STOP
        }
        startedByThisPress = true
        recording = true
        return Action.START
    }

    fun up(now: Long): Action {
        if (!startedByThisPress || !recording) return Action.NONE
        startedByThisPress = false
        if (now - pressedAt >= holdMillis) {
            recording = false
            return Action.STOP
        }
        return Action.NONE // a tap: keep recording until the next tap
    }

    /** Recording started without the button (hands-free): the next press stops it. */
    fun startedElsewhere() {
        recording = true
        startedByThisPress = false
    }

    /** Recording ended elsewhere (an error, the keyboard closed, silence). */
    fun reset() {
        recording = false
        startedByThisPress = false
    }
}

/** "670 MB", "1.2 GB" (decimal units, as phones show storage). */
fun formatSize(bytes: Long, locale: java.util.Locale = java.util.Locale.getDefault()): String = when {
    bytes >= 1_000_000_000 -> String.format(locale, "%.1f GB", bytes / 1e9)
    bytes >= 1_000_000 -> "${bytes / 1_000_000} MB"
    bytes >= 1_000 -> "${bytes / 1_000} kB"
    else -> "$bytes B"
}

/** Text with dictation put in at the selection, and where the cursor goes after it. */
data class Insertion(val text: String, val cursor: Int) {
    companion object {
        /**
         * [dictated] into [current] in place of the selection [start]..[end] (either -1: unknown,
         * so at the end), joined as [Typing.join] does.
         */
        fun of(current: String, start: Int, end: Int, dictated: String): Insertion {
            val from = if (start < 0 || end < 0) current.length else minOf(start, end).coerceIn(0, current.length)
            val to = if (start < 0 || end < 0) current.length else maxOf(start, end).coerceIn(0, current.length)
            val piece = Typing.join(current.substring(0, from), dictated)
            return Insertion(current.substring(0, from) + piece + current.substring(to), from + piece.length)
        }
    }
}
