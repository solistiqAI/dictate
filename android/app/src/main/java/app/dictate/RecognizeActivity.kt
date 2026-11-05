package app.dictate

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.speech.RecognizerIntent
import android.util.TypedValue
import android.view.Gravity
import android.widget.LinearLayout
import android.widget.TextView

/**
 * `RecognizerIntent.ACTION_RECOGNIZE_SPEECH`: apps that start the system's voice input as an
 * activity get a small Dictate dialog that listens at once, stops when the speaker does (or on a
 * tap) and returns the text.
 */
class RecognizeActivity : Activity() {
    private lateinit var status: TextView
    private lateinit var mic: MicButton
    private var listening: Listening? = null
    private var busy = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        status = TextView(this).apply {
            gravity = Gravity.CENTER
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
        }
        mic = MicButton(this, 72)
        mic.icon.setOnClickListener {
            if (listening != null) listening?.finish() else if (!busy) start()
        }
        val pad = (24 * resources.displayMetrics.density).toInt()
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER_HORIZONTAL
                setPadding(pad, pad, pad, pad)
                addView(status)
                addView(mic, LinearLayout.LayoutParams(pad * 5, pad * 5).apply { topMargin = pad / 2 })
            },
        )
        if (!Setup.micGranted(this) || !ModelDownload(this).installed()) {
            startActivity(Intent(this, MainActivity::class.java))
            setResult(RESULT_CANCELED)
            finish()
            return
        }
        Engine.prepare(this)
        start()
    }

    private fun start() {
        val listen = Listening(
            this,
            handsFree = true,
            object : Listening.Events {
                override fun level(value: Float) = mic.level(value)

                override fun finished(samples: FloatArray) {
                    listening = null
                    transcribe(samples)
                }
            },
        )
        if (!listen.start()) {
            status.setText(R.string.keyboard_mic_failed)
            return
        }
        listening = listen
        mic.show(MicButton.Mode.RECORDING)
        status.setText(R.string.keyboard_listening)
    }

    private fun transcribe(samples: FloatArray) {
        busy = true
        mic.show(MicButton.Mode.BUSY)
        status.setText(R.string.keyboard_transcribing)
        Engine.transcribe(this, samples, Recorder.RATE) { result ->
            busy = false
            val text = result.getOrNull().orEmpty()
            if (text.isBlank()) {
                mic.show(MicButton.Mode.IDLE)
                status.text = result.exceptionOrNull()?.let(Engine::describe) ?: getString(R.string.keyboard_nothing)
                return@transcribe
            }
            setResult(
                RESULT_OK,
                Intent()
                    .putStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS, arrayListOf(text))
                    .putExtra(RecognizerIntent.EXTRA_CONFIDENCE_SCORES, floatArrayOf(1f)),
            )
            finish()
        }
    }

    override fun onStop() {
        listening?.cancel()
        listening = null
        super.onStop()
    }
}
