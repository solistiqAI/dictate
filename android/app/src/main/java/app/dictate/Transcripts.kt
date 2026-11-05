package app.dictate

import java.io.File

/** One transcript in the list of recent ones. */
data class Transcript(val time: Long, val source: Source, val text: String) {
    enum class Source { KEYBOARD, MESSAGE }
}

/**
 * The recent transcripts, newest first, at most [limit]: a small text file in the app's own
 * folder, never anywhere else. One line per transcript: time, source, text (tabs, line breaks and
 * backslashes escaped).
 */
class TranscriptLog(private val file: File, private val limit: Int = 50) {
    @Synchronized
    fun all(): List<Transcript> {
        if (!file.exists()) return emptyList()
        return file.readLines().mapNotNull(::decode)
    }

    @Synchronized
    fun add(transcript: Transcript) {
        if (transcript.text.isBlank()) return
        val lines = listOf(encode(transcript)) + all().take(limit - 1).map(::encode)
        write(lines)
    }

    @Synchronized
    fun remove(transcript: Transcript) {
        write(all().filter { it != transcript }.map(::encode))
    }

    @Synchronized
    fun clear() {
        file.delete()
    }

    private fun write(lines: List<String>) {
        file.parentFile?.mkdirs()
        val part = File(file.path + ".part")
        part.writeText(lines.joinToString("") { it + "\n" })
        if (!part.renameTo(file)) {
            file.delete()
            part.renameTo(file)
        }
    }

    companion object {
        fun encode(t: Transcript): String = "${t.time}\t${t.source.name}\t${escape(t.text)}"

        fun decode(line: String): Transcript? {
            val parts = line.split('\t', limit = 3)
            if (parts.size != 3) return null
            val time = parts[0].toLongOrNull() ?: return null
            val source = Transcript.Source.entries.find { it.name == parts[1] } ?: return null
            return Transcript(time, source, unescape(parts[2]))
        }

        private fun escape(text: String): String = buildString {
            for (c in text) {
                when (c) {
                    '\\' -> append("\\\\")
                    '\t' -> append("\\t")
                    '\n' -> append("\\n")
                    '\r' -> append("\\r")
                    else -> append(c)
                }
            }
        }

        private fun unescape(text: String): String = buildString {
            var i = 0
            while (i < text.length) {
                val c = text[i]
                if (c == '\\' && i + 1 < text.length) {
                    when (text[i + 1]) {
                        't' -> append('\t')
                        'n' -> append('\n')
                        'r' -> append('\r')
                        else -> append(text[i + 1])
                    }
                    i += 2
                } else {
                    append(c)
                    i++
                }
            }
        }
    }
}
