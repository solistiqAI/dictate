package app.dictate

/** A growing buffer of mono samples, for the recorder and the decoder. */
class Samples(capacity: Int = 16_000 * 10) {
    private var data = FloatArray(capacity)
    var size = 0
        private set

    fun add(sample: Float) {
        if (size == data.size) data = data.copyOf(data.size * 2)
        data[size++] = sample
    }

    fun toArray(): FloatArray = data.copyOf(size)
}

/**
 * Averages every [factor] samples into one on the way into [out]: 48 kHz voice messages (Opus
 * always decodes at 48 kHz) at a third of the memory. A box filter is a crude low-pass, but
 * speech recognition does not mind; dictate-core resamples whatever rate comes.
 */
class Decimator(private val factor: Int, private val out: Samples) {
    private var sum = 0f
    private var count = 0

    fun add(sample: Float) {
        sum += sample
        if (++count == factor) {
            out.add(sum / factor)
            sum = 0f
            count = 0
        }
    }

    companion object {
        /** The factor that brings [rate] down to 16 kHz, 1 when it does not divide evenly. */
        fun factorFor(rate: Int): Int = if (rate > 16_000 && rate % 16_000 == 0) rate / 16_000 else 1
    }
}

object Pcm {
    /** 16-bit PCM to [-1, 1]. */
    fun toFloat(sample: Short): Float = sample / 32768f

    /** Interleaved 16-bit frames of [channels] channels, mixed down to mono, into [out]. */
    fun addInterleaved(samples: ShortArray, count: Int, channels: Int, out: Samples) =
        addInterleaved(samples, count, channels, out::add)

    fun addInterleaved(samples: ShortArray, count: Int, channels: Int, out: (Float) -> Unit) {
        require(channels >= 1) { "channels: $channels" }
        var i = 0
        while (i + channels <= count) {
            var sum = 0f
            for (c in 0 until channels) sum += toFloat(samples[i + c])
            out(sum / channels)
            i += channels
        }
    }

    /** The same for float PCM. */
    fun addInterleaved(samples: FloatArray, count: Int, channels: Int, out: Samples) =
        addInterleaved(samples, count, channels, out::add)

    fun addInterleaved(samples: FloatArray, count: Int, channels: Int, out: (Float) -> Unit) {
        require(channels >= 1) { "channels: $channels" }
        var i = 0
        while (i + channels <= count) {
            var sum = 0f
            for (c in 0 until channels) sum += samples[i + c]
            out(sum / channels)
            i += channels
        }
    }

    /** Loudness of a stretch of samples, 0 (silence) to 1, for the keyboard's level ring. */
    fun level(samples: ShortArray, count: Int): Float {
        if (count <= 0) return 0f
        var sum = 0.0
        for (i in 0 until count) {
            val s = toFloat(samples[i])
            sum += s * s
        }
        val rms = Math.sqrt(sum / count)
        // -60 dB … 0 dB to 0 … 1.
        val db = 20 * Math.log10(rms.coerceAtLeast(1e-6))
        return ((db + 60) / 60).toFloat().coerceIn(0f, 1f)
    }
}
