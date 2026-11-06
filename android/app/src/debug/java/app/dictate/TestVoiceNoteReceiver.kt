package app.dictate

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import java.io.File

/**
 * Debug build only: a pretend incoming WhatsApp voice message, to try the transcript notification
 * and the chat strip without anyone sending one. From a computer (adb holds DUMP, apps don't):
 *
 *     adb shell am broadcast -n app.dictate/.TestVoiceNoteReceiver \
 *         --es file /sdcard/Android/data/app.dictate/files/test.opus [--es sender Anna] [--es chat Anna]
 *
 * Without `chat` it goes to the WhatsApp chat open on screen. The file goes through the same
 * steps as a real voice message (VoiceNoteListener.handle).
 */
class TestVoiceNoteReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val path = intent.getStringExtra("file") ?: return
        val chat = intent.getStringExtra("chat") ?: FloatingMicService.instance?.openChatName() ?: "Test"
        val sender = intent.getStringExtra("sender") ?: chat
        val pending = goAsync()
        Thread {
            try {
                VoiceNoteListener.handle(context.applicationContext, File(path), sender, chat, sender, null)
            } finally {
                pending.finish()
            }
        }.start()
    }
}
