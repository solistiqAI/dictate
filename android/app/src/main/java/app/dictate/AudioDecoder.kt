package app.dictate

import android.content.Context
import android.media.AudioFormat
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import android.net.Uri
import java.nio.ByteOrder

/** Decoded audio: mono samples at [rate] Hz. */
class Decoded(val samples: FloatArray, val rate: Int) {
    val seconds: Float get() = if (rate > 0) samples.size.toFloat() / rate else 0f
}

/**
 * A shared voice message (Ogg/Opus from WhatsApp, AAC, MP3, WAV, …) to mono samples, with the
 * phone's own decoders. The model's sample rate comes later (dictate-core resamples).
 */
object AudioDecoder {
    private const val TIMEOUT_US = 10_000L

    /** Longer messages are cut here: the samples of 15 minutes at 48 kHz take 170 MB already. */
    const val MAX_SECONDS = 15 * 60

    fun decode(context: Context, uri: Uri): Decoded {
        val extractor = MediaExtractor()
        try {
            extractor.setDataSource(context, uri, null)
            val track = (0 until extractor.trackCount).firstOrNull {
                extractor.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("audio/") == true
            } ?: throw IllegalArgumentException("no audio in this file")
            extractor.selectTrack(track)
            val format = extractor.getTrackFormat(track)
            val mime = format.getString(MediaFormat.KEY_MIME)!!
            val codec = MediaCodec.createDecoderByType(mime)
            try {
                codec.configure(format, null, null, 0)
                codec.start()
                return drain(extractor, codec, format)
            } finally {
                runCatching { codec.stop() }
                codec.release()
            }
        } finally {
            extractor.release()
        }
    }

    private fun drain(extractor: MediaExtractor, codec: MediaCodec, input: MediaFormat): Decoded {
        val out = Samples()
        var rate = input.getInteger(MediaFormat.KEY_SAMPLE_RATE)
        var channels = input.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
        var encoding = AudioFormat.ENCODING_PCM_16BIT
        val info = MediaCodec.BufferInfo()
        var inputDone = false
        var shorts = ShortArray(0)
        var floats = FloatArray(0)
        while (true) {
            if (!inputDone) {
                val index = codec.dequeueInputBuffer(TIMEOUT_US)
                if (index >= 0) {
                    val buffer = codec.getInputBuffer(index)!!
                    val size = extractor.readSampleData(buffer, 0)
                    if (size < 0) {
                        codec.queueInputBuffer(index, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                        inputDone = true
                    } else {
                        codec.queueInputBuffer(index, 0, size, extractor.sampleTime, 0)
                        extractor.advance()
                    }
                }
            }
            val index = codec.dequeueOutputBuffer(info, TIMEOUT_US)
            when {
                index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    val format = codec.outputFormat
                    rate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                    channels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                    if (format.containsKey(MediaFormat.KEY_PCM_ENCODING)) {
                        encoding = format.getInteger(MediaFormat.KEY_PCM_ENCODING)
                    }
                }
                index >= 0 -> {
                    val buffer = codec.getOutputBuffer(index)!!.order(ByteOrder.nativeOrder())
                    buffer.position(info.offset)
                    buffer.limit(info.offset + info.size)
                    if (encoding == AudioFormat.ENCODING_PCM_FLOAT) {
                        val view = buffer.asFloatBuffer()
                        val n = view.remaining()
                        if (floats.size < n) floats = FloatArray(n)
                        view.get(floats, 0, n)
                        Pcm.addInterleaved(floats, n, channels, out)
                    } else {
                        val view = buffer.asShortBuffer()
                        val n = view.remaining()
                        if (shorts.size < n) shorts = ShortArray(n)
                        view.get(shorts, 0, n)
                        Pcm.addInterleaved(shorts, n, channels, out)
                    }
                    codec.releaseOutputBuffer(index, false)
                    if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) break
                    if (out.size > MAX_SECONDS.toLong() * rate) break
                }
            }
        }
        return Decoded(out.toArray(), rate)
    }
}
