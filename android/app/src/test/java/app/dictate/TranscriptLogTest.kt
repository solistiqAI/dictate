package app.dictate

import org.junit.Test
import java.io.File
import kotlin.io.path.createTempDirectory
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class TranscriptLogTest {
    private fun log(limit: Int = 50) = TranscriptLog(File(createTempDirectory("transcripts").toFile(), "t.txt"), limit)

    @Test
    fun newest_first_and_at_most_the_limit() {
        val log = log(limit = 3)
        assertTrue(log.all().isEmpty())
        for (i in 1..5) log.add(Transcript(i.toLong(), Transcript.Source.KEYBOARD, "Satz $i"))
        assertEquals(listOf("Satz 5", "Satz 4", "Satz 3"), log.all().map { it.text })
        log.add(Transcript(9, Transcript.Source.MESSAGE, "   "))
        assertEquals(3, log.all().size)
    }

    @Test
    fun line_breaks_tabs_and_backslashes_survive() {
        val log = log()
        val text = "Erste Zeile\nzweite\tmit Tab, C:\\Pfad und \\n wörtlich"
        val transcript = Transcript(1_700_000_000_000, Transcript.Source.MESSAGE, text)
        log.add(transcript)
        assertEquals(listOf(transcript), log.all())
        log.remove(transcript)
        assertTrue(log.all().isEmpty())
    }

    @Test
    fun broken_lines_are_skipped_and_clear_empties() {
        assertNull(TranscriptLog.decode("nonsense"))
        assertNull(TranscriptLog.decode("12\tVOICE\ttext"))
        val log = log()
        log.add(Transcript(1, Transcript.Source.KEYBOARD, "eins"))
        log.clear()
        assertTrue(log.all().isEmpty())
    }
}
