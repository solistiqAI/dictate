package app.dictate

import android.content.ContextParams
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.speech.RecognitionService
import android.speech.SpeechRecognizer

/**
 * Dictate as the system's speech recognizer (`android.speech.RecognitionService`): apps and
 * keyboards that ask `SpeechRecognizer` for voice input get Dictate when it is chosen as the
 * voice input / speech recognition service. Hands-free: listens until the speaker stops, then one
 * final result (no partial results yet).
 */
class DictateRecognizer : RecognitionService() {
    private var listening: Listening? = null
    private var busy = false

    override fun onStartListening(intent: Intent?, callback: Callback) {
        if (listening != null || busy) {
            callback.error(SpeechRecognizer.ERROR_RECOGNIZER_BUSY)
            return
        }
        if (!Setup.micGranted(this)) {
            callback.error(SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS)
            return
        }
        if (!ModelDownload(this).installed()) {
            callback.error(SpeechRecognizer.ERROR_SERVER)
            return
        }
        Engine.prepare(this)
        // Record on behalf of the app that asked (Android 12+: its attribution, its mic indicator).
        val recordAs = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            createContext(ContextParams.Builder().setNextAttributionSource(callback.callingAttributionSource).build())
        } else {
            this
        }
        val listen = Listening(
            recordAs,
            handsFree = true,
            object : Listening.Events {
                override fun level(value: Float) {
                    // RMS in dB, roughly the range recognizers report (about -2 to 10).
                    callback.rmsChanged(value * 12f - 2f)
                }

                override fun speechStarted() {
                    callback.beginningOfSpeech()
                }

                override fun finished(samples: FloatArray) {
                    listening = null
                    callback.endOfSpeech()
                    transcribe(samples, callback)
                }
            },
        )
        if (!listen.start()) {
            callback.error(SpeechRecognizer.ERROR_AUDIO)
            return
        }
        listening = listen
        callback.readyForSpeech(Bundle())
    }

    private fun transcribe(samples: FloatArray, callback: Callback) {
        busy = true
        Engine.transcribe(this, samples, Recorder.RATE) { result ->
            busy = false
            result.onSuccess { text ->
                if (text.isBlank()) {
                    callback.error(SpeechRecognizer.ERROR_NO_MATCH)
                } else {
                    callback.results(
                        Bundle().apply {
                            putStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION, arrayListOf(text))
                            putFloatArray(SpeechRecognizer.CONFIDENCE_SCORES, floatArrayOf(1f))
                        },
                    )
                }
            }.onFailure { callback.error(SpeechRecognizer.ERROR_CLIENT) }
        }
    }

    override fun onStopListening(callback: Callback) {
        listening?.finish()
    }

    override fun onCancel(callback: Callback) {
        listening?.cancel()
        listening = null
    }

    override fun onDestroy() {
        listening?.cancel()
        listening = null
        super.onDestroy()
    }
}
