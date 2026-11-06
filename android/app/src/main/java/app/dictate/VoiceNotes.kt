package app.dictate

import java.io.File

/**
 * The pure part of transcribing incoming WhatsApp voice messages by themselves: which
 * notification is a voice message, and which file in WhatsApp's folder belongs to it. Tested on
 * the JVM; [VoiceNoteListener] does the rest.
 */
object VoiceNotes {
    /** WhatsApp and WhatsApp Business. */
    val PACKAGES = setOf("com.whatsapp", "com.whatsapp.w4b")

    /**
     * WhatsApp keeps received (and sent) voice messages here, one folder per week
     * (`WhatsApp Voice Notes/202645/PTT-20261106-WA0003.opus`), below the shared storage's root.
     */
    val FOLDERS = listOf(
        "Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Voice Notes",
        "Android/media/com.whatsapp.w4b/WhatsApp Business/Media/WhatsApp Business Voice Notes",
    )

    /** The duration WhatsApp writes after a voice message's text, "(0:12)", "(1:05:03)". */
    private val duration = Regex("""\((?:(\d+):)?(\d{1,2}):(\d{2})\)""")

    /**
     * A notification's message text is a voice message: WhatsApp writes "🎤 Sprachnachricht (0:12)",
     * "🎤 Voice message (0:12)" (the microphone and the duration in any language).
     */
    fun isVoiceMessage(text: CharSequence?): Boolean {
        val value = text?.toString() ?: return false
        return value.contains("🎤") && duration.containsMatchIn(value)
    }

    /** The duration in the message's text, in seconds, or null. */
    fun seconds(text: CharSequence?): Int? {
        val match = duration.find(text ?: return null) ?: return null
        val (hours, minutes, seconds) = match.destructured
        return (hours.toIntOrNull() ?: 0) * 3600 + minutes.toInt() * 60 + seconds.toInt()
    }

    /**
     * The voice message file that arrived for a notification: the oldest one not [taken] yet,
     * changed at or after [since] (ms), in the newest two week folders below [roots]. Oldest first,
     * so several messages at once go in order.
     */
    fun newFile(roots: List<File>, since: Long, taken: Set<String>): File? =
        roots.asSequence()
            .flatMap { root -> weeks(root).asSequence() }
            .flatMap { folder -> (folder.listFiles() ?: emptyArray()).asSequence() }
            .filter { it.isFile && it.name.endsWith(".opus") && it.length() > 0 }
            .filter { it.lastModified() >= since && it.path !in taken }
            .minByOrNull { it.lastModified() }

    /** The two newest week folders (plus the folder itself, for files right in it). */
    private fun weeks(root: File): List<File> {
        val folders = root.listFiles { file -> file.isDirectory }?.sortedByDescending { it.name }?.take(2) ?: emptyList()
        return folders + root
    }

    /** The text of the transcript notification and of the recent transcripts: who, then what. */
    fun line(sender: CharSequence?, text: String): String =
        if (sender.isNullOrBlank()) text else "$sender: $text"
}
