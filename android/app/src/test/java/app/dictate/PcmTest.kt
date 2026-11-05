package app.dictate

import org.junit.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class PcmTest {
    @Test
    fun stereo_is_mixed_down_to_mono() {
        val out = Samples(capacity = 1)
        Pcm.addInterleaved(shortArrayOf(16384, 0, -32768, -32768, 100), 5, 2, out)
        // The odd sample at the end is half a frame: dropped.
        assertContentEquals(floatArrayOf(0.25f, -1f), out.toArray())
        Pcm.addInterleaved(floatArrayOf(0.5f, 0.5f, 1f, 0f), 4, 2, out)
        assertContentEquals(floatArrayOf(0.25f, -1f, 0.5f, 0.5f), out.toArray())
        assertEquals(4, out.size)
    }

    @Test
    fun the_buffer_grows() {
        val out = Samples(capacity = 2)
        repeat(1000) { out.add(it.toFloat()) }
        val all = out.toArray()
        assertEquals(1000, all.size)
        assertEquals(999f, all.last())
    }

    @Test
    fun level_is_zero_for_silence_and_near_one_for_full_scale() {
        assertEquals(0f, Pcm.level(ShortArray(1600), 1600))
        assertEquals(0f, Pcm.level(ShortArray(0), 0))
        val loud = ShortArray(1600) { if (it % 2 == 0) 30000 else -30000 }
        assertTrue(Pcm.level(loud, loud.size) > 0.95f)
        val quiet = ShortArray(1600) { if (it % 2 == 0) 30 else -30 }
        assertTrue(Pcm.level(quiet, quiet.size) < 0.1f)
    }
}
