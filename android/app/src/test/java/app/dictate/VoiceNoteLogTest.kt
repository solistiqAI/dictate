package app.dictate

import org.junit.Test
import java.nio.file.Files
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class VoiceNoteLogTest {
    @Test
    fun notes_come_back_newest_first_per_chat() {
        val dir = Files.createTempDirectory("notes").toFile()
        try {
            val log = VoiceNoteLog(java.io.File(dir, "voice_notes.txt"))
            log.add(VoiceNote(1_000, "Anna", "Anna", "Erste\tmit Tab"))
            log.add(VoiceNote(2_000, "Familie", "Ben", "In der Gruppe\nzwei Zeilen"))
            log.add(VoiceNote(3_000, "Anna", "Anna", "Zweite"))
            assertEquals(listOf("Zweite", "Erste\tmit Tab"), log.forChat("anna", since = 0).map { it.text })
            assertEquals(listOf("Zweite"), log.forChat("Anna", since = 2_500).map { it.text })
            assertEquals("In der Gruppe\nzwei Zeilen", log.forChat("Familie", since = 0).single().text)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun chat_names_match_without_direction_marks_and_spaces() {
        assertTrue(VoiceNoteLog.sameChat("\u202A+49 170 1234567\u202C", "+491701234567"))
        assertTrue(VoiceNoteLog.sameChat("Anna Example", "anna example"))
        assertFalse(VoiceNoteLog.sameChat("Anna", "Ben"))
        assertFalse(VoiceNoteLog.sameChat("", ""))
    }
}
