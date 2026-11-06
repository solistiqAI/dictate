package app.dictate

import java.io.File

/** One transcribed incoming voice message: when, in which chat, from whom, what. */
data class VoiceNote(val time: Long, val chat: String, val sender: String, val text: String)

/**
 * The transcribed WhatsApp voice messages, newest first, at most [limit], for the strip over an
 * open chat ([ChatStrip]): a small text file in the app's own folder. One line each: time, chat,
 * sender, text (tabs, line breaks and backslashes escaped as in [TranscriptLog]).
 */
class VoiceNoteLog(private val file: File, private val limit: Int = 300) {
    @Synchronized
    fun all(): List<VoiceNote> {
        if (!file.exists()) return emptyList()
        return file.readLines().mapNotNull(::decode)
    }

    @Synchronized
    fun add(note: VoiceNote) {
        if (note.text.isBlank()) return
        val lines = listOf(encode(note)) + all().take(limit - 1).map(::encode)
        file.parentFile?.mkdirs()
        val part = File(file.path + ".part")
        part.writeText(lines.joinToString("") { it + "\n" })
        if (!part.renameTo(file)) {
            file.delete()
            part.renameTo(file)
        }
    }

    /** The chat's voice messages since [since] (ms), newest first. */
    fun forChat(chat: String, since: Long): List<VoiceNote> =
        all().filter { it.time >= since && sameChat(it.chat, chat) }

    @Synchronized
    fun clear() {
        file.delete()
    }

    companion object {
        fun encode(n: VoiceNote): String =
            listOf(n.time.toString(), n.chat, n.sender, n.text).joinToString("\t") { TranscriptLog.escape(it) }

        fun decode(line: String): VoiceNote? {
            val parts = line.split('\t', limit = 4)
            if (parts.size != 4) return null
            val time = parts[0].toLongOrNull() ?: return null
            return VoiceNote(time, TranscriptLog.unescape(parts[1]), TranscriptLog.unescape(parts[2]), TranscriptLog.unescape(parts[3]))
        }

        /**
         * The same chat in a notification and in WhatsApp's header: names compared without case,
         * spaces and the direction marks WhatsApp puts around phone numbers.
         */
        fun sameChat(a: String, b: String): Boolean = key(a).isNotEmpty() && key(a) == key(b)

        private fun key(name: String): String =
            name.filterNot { it.isWhitespace() || it in DIRECTION_MARKS }.lowercase()

        private const val DIRECTION_MARKS = "\u200E\u200F\u202A\u202B\u202C\u202D\u202E\u2066\u2067\u2068\u2069"
    }
}
