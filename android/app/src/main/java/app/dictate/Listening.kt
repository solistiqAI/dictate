package app.dictate

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock

/**
 * When hands-free listening has heard enough: speech, then [silenceMillis] of quiet; or nothing
 * but quiet for [giveUpMillis]. Fed the recorder's level (0 to 1, -60 to 0 dB) with the time since
 * the start; the quiet is measured against the quietest level so far, so a noisy room works too.
 */
class SilenceStop(
    private val silenceMillis: Long = 1500,
    private val giveUpMillis: Long = 8000,
    private val margin: Float = 0.2f,
    private val minimum: Float = 0.25f,
) {
    enum class Event { NONE, SPEECH_STARTED, STOP }

    private var floor = 1f
    private var lastSpeech = 0L

    var heardSpeech = false
        private set

    fun feed(level: Float, at: Long): Event {
        // The floor follows the quietest moment, and creeps up slowly if the room gets louder.
        floor = minOf(level, floor + 0.0005f)
        if (level > floor + margin && level > minimum) {
            lastSpeech = at
            if (!heardSpeech) {
                heardSpeech = true
                return Event.SPEECH_STARTED
            }
        }
        if (heardSpeech && at - lastSpeech >= silenceMillis) return Event.STOP
        if (!heardSpeech && at >= giveUpMillis) return Event.STOP
        return Event.NONE
    }
}

/**
 * One recording from start to samples: the level for a meter, and (hands-free) the end of speech
 * by itself. Callbacks on the main thread.
 */
class Listening(
    private val context: Context,
    private val handsFree: Boolean,
    private val events: Events,
) {
    interface Events {
        fun level(value: Float) {}
        fun speechStarted() {}
        fun finished(samples: FloatArray)
    }

    private val recorder = Recorder()
    private val main = Handler(Looper.getMainLooper())
    private val silence = SilenceStop()
    private var startedAt = 0L

    var active = false
        private set

    private val tick = object : Runnable {
        override fun run() {
            if (!active) return
            val level = recorder.level
            events.level(level)
            when (silence.feed(level, SystemClock.uptimeMillis() - startedAt)) {
                SilenceStop.Event.SPEECH_STARTED -> events.speechStarted()
                SilenceStop.Event.STOP -> if (handsFree) {
                    finish()
                    return
                }
                SilenceStop.Event.NONE -> {}
            }
            main.postDelayed(this, 50)
        }
    }

    /** False when the microphone does not start (no permission, in use). */
    fun start(): Boolean {
        if (!recorder.start(context)) return false
        active = true
        startedAt = SystemClock.uptimeMillis()
        main.post(tick)
        return true
    }

    /** Stops and hands over what was recorded. */
    fun finish() {
        if (!active) return
        active = false
        main.removeCallbacks(tick)
        events.finished(recorder.stop())
    }

    /** Stops and drops the recording. */
    fun cancel() {
        if (!active) return
        active = false
        main.removeCallbacks(tick)
        recorder.stop()
    }
}
