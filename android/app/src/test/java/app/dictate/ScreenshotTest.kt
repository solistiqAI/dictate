package app.dictate

import android.view.ViewGroup
import androidx.activity.ComponentActivity
import app.dictate.ui.DictateTheme
import app.dictate.ui.MainScreen
import app.dictate.ui.MainState
import app.dictate.ui.MessageState
import app.dictate.ui.ModelState
import app.dictate.ui.ShareScreen
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The screens rendered on the JVM (Robolectric + Roborazzi), with invented data:
 * `./gradlew recordRoborazziDebug` writes them to `app/build/outputs/roborazzi/`.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w400dp-h880dp-xxhdpi")
class ScreenshotTest {
    private val firstStart = MainState(
        micGranted = false,
        model = ModelState.Missing(bytes = 670_310_000, metered = true),
        keyboardEnabled = false,
        keyboardChosen = false,
        transcripts = emptyList(),
        words = listOf("Claude", "Codex", "GitHub", "Pull Request", "Commit", "Deploy"),
    )

    private val ready = MainState(
        micGranted = true,
        model = ModelState.Ready,
        keyboardEnabled = true,
        keyboardChosen = true,
        transcripts = listOf(
            Transcript(1_762_350_000_000, Transcript.Source.MESSAGE, "Kannst du mir bitte die Rechnung von letzter Woche noch einmal schicken?"),
            Transcript(1_762_340_000_000, Transcript.Source.KEYBOARD, "Bin in zehn Minuten da, fang schon mal ohne mich an."),
        ),
        words = listOf("Claude", "GitHub", "Pull Request", "Wombat"),
    )

    @Test
    fun main_first_start() = captureRoboImage("$DIR/main-first-start.png") {
        DictateTheme(dynamic = false) { MainScreen(firstStart) }
    }

    @Test
    fun main_downloading() = captureRoboImage("$DIR/main-downloading.png") {
        DictateTheme(dynamic = false) {
            MainScreen(firstStart.copy(micGranted = true, model = ModelState.Downloading(212_000_000, 670_310_000)))
        }
    }

    @Test
    @Config(qualifiers = "+de")
    fun main_ready_german() = captureRoboImage("$DIR/main-ready-de.png") {
        DictateTheme(dynamic = false) { MainScreen(ready) }
    }

    @Test
    @Config(qualifiers = "+night")
    fun main_ready_dark() = captureRoboImage("$DIR/main-ready-dark.png") {
        DictateTheme(dark = true, dynamic = false) { MainScreen(ready) }
    }

    @Test
    fun share_two_messages() = captureRoboImage("$DIR/share-two-messages.png") {
        DictateTheme(dynamic = false) {
            ShareScreen(
                listOf(
                    MessageState.Done("Hi, I'm running late. Can we move the call to half past four? Thanks!"),
                    MessageState.Transcribing(48f),
                ),
                needsModel = false,
            )
        }
    }

    @Test
    fun keyboard() = captureKeyboard("$DIR/keyboard.png")

    @Test
    @Config(qualifiers = "+night")
    fun keyboard_dark() = captureKeyboard("$DIR/keyboard-dark.png")

    private fun captureKeyboard(path: String) {
        val activity = Robolectric.buildActivity(ComponentActivity::class.java).setup().get()
        val keyboard = KeyboardView(activity, NoKeys)
        activity.setContentView(
            keyboard,
            ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT),
        )
        keyboard.captureRoboImage(path)
    }

    private object NoKeys : KeyboardView.Listener {
        override fun onMicDown() {}
        override fun onMicUp() {}
        override fun onBackspace() {}
        override fun onSpace() {}
        override fun onEnter() {}
        override fun onSwitch() {}
        override fun onSwitchLong() {}
    }

    companion object {
        const val DIR = "build/outputs/roborazzi"
    }
}
