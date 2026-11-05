package app.dictate

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.mutableStateListOf
import app.dictate.ui.DictateTheme
import app.dictate.ui.MessageState
import app.dictate.ui.ShareActions
import app.dictate.ui.ShareScreen

/**
 * Share target for voice messages (WhatsApp: long-press a voice message, Share, Dictate): each
 * shared audio file becomes a text, in order, with Copy and Share.
 */
class ShareActivity : ComponentActivity() {
    private val messages = mutableStateListOf<MessageState>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val uris = sharedAudio(intent)
        val needsModel = !ModelDownload(this).installed()
        messages.addAll(uris.map { MessageState.Waiting })
        if (!needsModel) {
            VoiceMessages(this, uris) { index, state -> messages[index] = state }.start()
        }
        val actions = ShareActions(
            copy = { text ->
                Setup.copy(this, text)
                if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
                    Toast.makeText(this, R.string.copied, Toast.LENGTH_SHORT).show()
                }
            },
            share = { Setup.share(this, it) },
            openApp = {
                startActivity(Intent(this, MainActivity::class.java))
                finish()
            },
            close = { finish() },
        )
        setContent { DictateTheme { ShareScreen(messages, needsModel, actions) } }
    }

    /** The shared files: one (SEND) or several (SEND_MULTIPLE), in the order the app gave them. */
    private fun sharedAudio(intent: Intent): List<Uri> {
        val uris = when (intent.action) {
            Intent.ACTION_SEND -> listOfNotNull(stream(intent))
            Intent.ACTION_SEND_MULTIPLE -> streams(intent)
            else -> emptyList()
        }
        if (uris.isNotEmpty()) return uris
        // Some apps put the files only in the clip data.
        val clip = intent.clipData ?: return emptyList()
        return (0 until clip.itemCount).mapNotNull { clip.getItemAt(it).uri }
    }

    private fun stream(intent: Intent): Uri? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(Intent.EXTRA_STREAM)
        }

    private fun streams(intent: Intent): List<Uri> =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM).orEmpty()
        }
}
