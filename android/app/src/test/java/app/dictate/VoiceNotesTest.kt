package app.dictate

import org.junit.Test
import java.io.File
import java.nio.file.Files
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class VoiceNotesTest {
    @Test
    fun voice_messages_are_the_microphone_with_a_duration() {
        assertTrue(VoiceNotes.isVoiceMessage("🎤 Sprachnachricht (0:12)"))
        assertTrue(VoiceNotes.isVoiceMessage("🎤 Voice message (1:05)"))
        assertFalse(VoiceNotes.isVoiceMessage("Hallo, wie geht's? 🎤"))
        assertFalse(VoiceNotes.isVoiceMessage("📷 Foto"))
        assertFalse(VoiceNotes.isVoiceMessage(null))
    }

    @Test
    fun the_duration_is_read_in_seconds() {
        assertEquals(12, VoiceNotes.seconds("🎤 Sprachnachricht (0:12)"))
        assertEquals(65, VoiceNotes.seconds("🎤 Voice message (1:05)"))
        assertEquals(3723, VoiceNotes.seconds("🎤 (1:02:03)"))
        assertNull(VoiceNotes.seconds("Hallo"))
    }

    @Test
    fun the_oldest_new_file_not_taken_is_found_in_the_newest_weeks() {
        val root = Files.createTempDirectory("voice").toFile()
        try {
            val old = File(root, "202601").apply { mkdirs() }
            val week = File(root, "202645").apply { mkdirs() }
            File(old, "PTT-old.opus").apply { writeText("x"); setLastModified(1_000) }
            val first = File(week, "PTT-1.opus").apply { writeText("x"); setLastModified(10_000) }
            val second = File(week, "PTT-2.opus").apply { writeText("x"); setLastModified(11_000) }
            File(week, "PTT-3.opus.tmp").apply { writeText("x"); setLastModified(12_000) }
            File(week, "PTT-empty.opus").apply { createNewFile(); setLastModified(12_000) }

            assertEquals(first, VoiceNotes.newFile(listOf(root), since = 5_000, taken = emptySet()))
            assertEquals(second, VoiceNotes.newFile(listOf(root), since = 5_000, taken = setOf(first.path)))
            assertNull(VoiceNotes.newFile(listOf(root), since = 20_000, taken = emptySet()))
            assertNull(VoiceNotes.newFile(listOf(File(root, "missing")), since = 0, taken = emptySet()))
        } finally {
            root.deleteRecursively()
        }
    }

    @Test
    fun the_line_names_the_sender() {
        assertEquals("Anna: Hallo", VoiceNotes.line("Anna", "Hallo"))
        assertEquals("Hallo", VoiceNotes.line(null, "Hallo"))
    }
}
