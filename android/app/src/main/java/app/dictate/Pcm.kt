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

object Pcm {
    /** 16-bit PCM to [-1, 1]. */
    fun toFloat(sample: Short): Float = sample / 32768f

    /** Interleaved 16-bit frames of [channels] channels, mixed down to mono, into [out]. */
    fun addInterleaved(samples: ShortArray, count: Int, channels: Int, out: Samples) {
        require(channels >= 1) { "channels: $channels" }
        var i = 0
        while (i + channels <= count) {
            var sum = 0f
            for (c in 0 until channels) sum += toFloat(samples[i + c])
            out.add(sum / channels)
            i += channels
        }
    }

    /** The same for float PCM. */
    fun addInterleaved(samples: FloatArray, count: Int, channels: Int, out: Samples) {
        require(channels >= 1) { "channels: $channels" }
        var i = 0
        while (i + channels <= count) {
            var sum = 0f
            for (c in 0 until channels) sum += samples[i + c]
            out.add(sum / channels)
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
