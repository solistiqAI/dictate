package app.dictate

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import app.dictate.ui.MessageState
import java.util.concurrent.Executors

/**
 * Shared voice messages, one after the other: decode on one thread, transcribe on the engine's,
 * report each step on the main thread. Each finished one goes into the recent transcripts.
 */
class VoiceMessages(
    context: Context,
    private val uris: List<Uri>,
    private val update: (index: Int, state: MessageState) -> Unit,
) {
    private val app = context.applicationContext
    private val decoder = Executors.newSingleThreadExecutor { Thread(it, "dictate-decoder") }
    private val main = Handler(Looper.getMainLooper())

    fun start() {
        uris.forEachIndexed { index, uri ->
            decoder.execute {
                post(index, MessageState.Decoding)
                val decoded = try {
                    AudioDecoder.decode(app, uri)
                } catch (e: Throwable) { // OutOfMemoryError included: say so instead of crashing
                    post(index, MessageState.Failed(Engine.describe(e)))
                    return@execute
                }
                post(index, MessageState.Transcribing(decoded.seconds))
                Engine.transcribe(app, decoded.samples, decoded.rate) { result ->
                    val state = result.fold(
                        onSuccess = { text ->
                            if (text.isNotBlank()) {
                                AppFiles(app).transcripts.add(
                                    Transcript(System.currentTimeMillis(), Transcript.Source.MESSAGE, text),
                                )
                            }
                            MessageState.Done(text)
                        },
                        onFailure = { MessageState.Failed(Engine.describe(it)) },
                    )
                    update(index, state)
                }
            }
        }
        decoder.shutdown()
    }

    private fun post(index: Int, state: MessageState) {
        main.post { update(index, state) }
    }
}
