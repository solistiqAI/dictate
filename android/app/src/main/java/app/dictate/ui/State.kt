package app.dictate.ui

import app.dictate.StartWith
import app.dictate.Transcript

/** What the speech model's card shows. */
sealed interface ModelState {
    /** Not downloaded: its size ([bytes], null while unknown) and whether the network is metered. */
    data class Missing(val bytes: Long?, val metered: Boolean) : ModelState

    data class Downloading(val done: Long, val total: Long) : ModelState

    data class Failed(val reason: String) : ModelState

    data object Ready : ModelState
}

/** Everything the app screen shows; the activity fills it, the screenshot tests invent it. */
data class MainState(
    val micGranted: Boolean,
    val model: ModelState,
    val startWith: StartWith,
    /** The floating button's accessibility service is on. */
    val buttonOn: Boolean,
    /** Dictate (voice) is an enabled input method. */
    val keyboardEnabled: Boolean,
    /** What other keyboards' mic key opens: its name, and whether that is Dictate; null: nothing. */
    val micKeyOpens: Pair<String, Boolean>?,
    val transcripts: List<Transcript>,
    val words: List<String>,
    /** The last transcription: seconds of audio and of work, for measuring on the phone. */
    val timing: Pair<Float, Float>? = null,
)

/** One shared voice message on its way to text. */
sealed interface MessageState {
    data object Waiting : MessageState

    data object Decoding : MessageState

    data class Transcribing(val seconds: Float) : MessageState

    data class Done(val text: String) : MessageState

    data class Failed(val reason: String) : MessageState
}
