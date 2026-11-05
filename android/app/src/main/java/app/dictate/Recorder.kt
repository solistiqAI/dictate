package app.dictate

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder

/** One recording from the microphone: 16 kHz mono, as the model takes it. */
class Recorder {
    private val samples = Samples()
    private var thread: Thread? = null

    @Volatile
    private var running = false

    /** Loudness of the last tenth of a second, 0 to 1. */
    @Volatile
    var level = 0f
        private set

    /** False when the microphone is not allowed or does not start. */
    fun start(context: Context): Boolean {
        if (context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            return false
        }
        val min = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (min <= 0) return false
        val record = try {
            AudioRecord(
                // Tuned for speech recognition: no automatic gain, no noise suppression.
                MediaRecorder.AudioSource.VOICE_RECOGNITION,
                RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
                maxOf(min, RATE / 2 * 2),
            )
        } catch (e: SecurityException) {
            return false
        } catch (e: IllegalArgumentException) {
            return false
        }
        if (record.state != AudioRecord.STATE_INITIALIZED) {
            record.release()
            return false
        }
        try {
            record.startRecording()
        } catch (e: IllegalStateException) {
            record.release()
            return false
        }
        running = true
        thread = Thread({ read(record) }, "dictate-recorder").apply { start() }
        return true
    }

    private fun read(record: AudioRecord) {
        val buffer = ShortArray(RATE / 10)
        try {
            while (running && samples.size < MAX_SAMPLES) {
                val n = record.read(buffer, 0, buffer.size)
                if (n < 0) break
                synchronized(samples) { Pcm.addInterleaved(buffer, n, 1, samples) }
                level = Pcm.level(buffer, n)
            }
        } finally {
            runCatching { record.stop() }
            record.release()
        }
    }

    /** Stops and returns what was recorded. */
    fun stop(): FloatArray {
        running = false
        thread?.join(2000)
        thread = null
        return synchronized(samples) { samples.toArray() }
    }

    companion object {
        const val RATE = 16_000

        /** Ten minutes; a dictation is never that long. */
        const val MAX_SAMPLES = RATE * 600
    }
}
