package app.dictate

import org.junit.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFalse

class ListeningTest {
    private fun run(stop: SilenceStop, levels: List<Pair<Long, Float>>): List<Pair<Long, SilenceStop.Event>> =
        levels.map { (at, level) -> at to stop.feed(level, at) }.filter { it.second != SilenceStop.Event.NONE }

    @Test
    fun stops_after_speech_and_a_pause() {
        val quiet = 0.1f
        val voice = 0.55f
        val levels = (0L..6000L step 50).map { at -> at to if (at in 1000..2500) voice else quiet }
        val events = run(SilenceStop(silenceMillis = 1500), levels)
        assertEquals(1000L to SilenceStop.Event.SPEECH_STARTED, events.first())
        // The first STOP comes 1.5 s after the last loud moment (2500 ms).
        assertEquals(4000L to SilenceStop.Event.STOP, events[1])
    }

    @Test
    fun gives_up_when_nobody_speaks_and_ignores_a_steady_noise() {
        val stop = SilenceStop(giveUpMillis = 8000)
        val noisy = (0L..9000L step 50).map { at -> at to 0.4f }
        val events = run(stop, noisy)
        assertFalse(stop.heardSpeech)
        assertEquals(8000L to SilenceStop.Event.STOP, events.first())
    }

    @Test
    fun dictation_goes_in_at_the_selection() {
        assertEquals(Insertion("Hallo Welt", 10), Insertion.of("Hallo", 5, 5, "Welt"))
        assertEquals(Insertion("Hallo Welt!", 10), Insertion.of("Hallo!", 5, 5, "Welt"))
        assertEquals(Insertion("Welt", 4), Insertion.of("", 0, 0, "Welt"))
        // A selection is replaced; unknown selection: at the end.
        assertEquals(Insertion("Hallo Welt", 10), Insertion.of("Hallo Leute", 6, 11, "Welt"))
        assertEquals(Insertion("Hallo Welt", 10), Insertion.of("Hallo", -1, -1, "Welt"))
        assertEquals(Insertion("Hallo Welt", 10), Insertion.of("Hallo", 99, 99, "Welt"))
    }

    @Test
    fun voice_messages_at_48_khz_are_kept_at_16() {
        assertEquals(3, Decimator.factorFor(48_000))
        assertEquals(1, Decimator.factorFor(44_100))
        assertEquals(1, Decimator.factorFor(16_000))
        val out = Samples(capacity = 1)
        val decimator = Decimator(3, out)
        floatArrayOf(0.25f, 0.5f, 0.75f, 1f, 1f, 1f, 0.5f).forEach(decimator::add)
        // Two groups of three, averaged; the seventh sample waits for its group.
        assertContentEquals(floatArrayOf(0.5f, 1f), out.toArray())
    }
}
