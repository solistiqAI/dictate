package app.dictate

import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class LiveTest {
    @Test
    fun short_text_stays_whole() {
        assertEquals("Hallo Welt", Live.tail("  Hallo Welt "))
    }

    @Test
    fun long_text_keeps_the_newest_words_whole() {
        val text = (1..60).joinToString(" ") { "wort$it" }
        val tail = Live.tail(text, max = 40)
        assertTrue(tail.startsWith("…wort"))
        assertTrue(tail.endsWith("wort60"))
        assertTrue(tail.length <= 41)
        // No word cut in half after the "…".
        assertTrue(tail.removePrefix("…").split(" ").all { it.startsWith("wort") })
    }

    @Test
    fun passes_wait_the_interval_or_twice_the_last_pass() {
        assertEquals(Live.INTERVAL_MILLIS, Live.next(50))
        assertEquals(2000L, Live.next(1000))
    }
}
