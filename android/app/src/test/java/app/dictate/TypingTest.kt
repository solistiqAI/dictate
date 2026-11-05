package app.dictate

import org.junit.Test
import java.util.Locale
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class TypingTest {
    @Test
    fun dictated_text_gets_a_space_between_words_only() {
        assertEquals("Hallo", Typing.join(null, "Hallo"))
        assertEquals("Hallo", Typing.join("", "Hallo"))
        assertEquals(" Welt", Typing.join("Hallo", "Welt"))
        assertEquals("Welt", Typing.join("Hallo ", "Welt"))
        assertEquals("Welt", Typing.join("Hallo\n", "Welt"))
        assertEquals("Welt", Typing.join("(", "Welt"))
        assertEquals(", oder?", Typing.join("Ja", ", oder?"))
        assertEquals(".", Typing.join("Ende", "."))
        assertEquals("", Typing.join("Hallo", ""))
    }

    @Test
    fun holding_the_button_talks_until_release() {
        val mic = MicGesture(holdMillis = 400)
        assertEquals(MicGesture.Action.START, mic.down(1_000))
        assertTrue(mic.recording)
        assertEquals(MicGesture.Action.STOP, mic.up(2_500))
        assertFalse(mic.recording)
    }

    @Test
    fun a_tap_starts_and_the_next_tap_stops() {
        val mic = MicGesture(holdMillis = 400)
        assertEquals(MicGesture.Action.START, mic.down(1_000))
        assertEquals(MicGesture.Action.NONE, mic.up(1_100))
        assertTrue(mic.recording)
        assertEquals(MicGesture.Action.STOP, mic.down(5_000))
        assertFalse(mic.recording)
        // Releasing the stopping tap does nothing more.
        assertEquals(MicGesture.Action.NONE, mic.up(5_050))
        // And the next press starts again.
        assertEquals(MicGesture.Action.START, mic.down(6_000))
        mic.reset()
        assertFalse(mic.recording)
        assertEquals(MicGesture.Action.NONE, mic.up(9_000))
    }

    @Test
    fun sizes_read_as_phones_show_them() {
        assertEquals("670 MB", formatSize(670_123_456))
        assertEquals("2.5 GB", formatSize(2_500_000_000, Locale.ENGLISH))
        assertEquals("2,5 GB", formatSize(2_500_000_000, Locale.GERMAN))
        assertEquals("12 kB", formatSize(12_345))
        assertEquals("5 B", formatSize(5))
    }
}
