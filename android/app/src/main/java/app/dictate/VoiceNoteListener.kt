package app.dictate

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.Parcelable
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import android.util.Log
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger

/**
 * Incoming WhatsApp voice messages to text, without sharing them: when WhatsApp's notification
 * says a voice message came in, the file WhatsApp downloads into its voice notes folder goes
 * through the model, and a notification of our own shows who said what (tap: the chat; Copy).
 * Only WhatsApp's notifications are looked at, nothing of them is kept but the transcript (in the
 * recent transcripts, on the phone). Needs notification access and access to all files (WhatsApp's
 * folder has a .nomedia, so the media store doesn't list it).
 */
class VoiceNoteListener : NotificationListenerService() {
    private val worker = Executors.newSingleThreadExecutor { Thread(it, "dictate-voice-notes") }
    private val seen = LinkedHashSet<String>()
    private val taken = LinkedHashSet<String>()

    private class Message(val text: CharSequence?, val sender: CharSequence?, val time: Long)

    override fun onNotificationPosted(sbn: StatusBarNotification) {
        if (sbn.packageName !in VoiceNotes.PACKAGES) return
        if (!Settings(this).voiceNotes) return
        val notification = sbn.notification ?: return
        if (notification.flags and Notification.FLAG_GROUP_SUMMARY != 0) return
        val message = lastMessage(notification, sbn.postTime)
        if (!VoiceNotes.isVoiceMessage(message.text)) return
        // WhatsApp posts a chat's notification again with every new message: each one once.
        val key = "${sbn.key}|${message.time}|${message.text}"
        synchronized(seen) {
            if (!seen.add(key)) return
            while (seen.size > 200) seen.remove(seen.first())
        }
        if (!Setup.allFiles() || !ModelDownload(this).installed()) {
            Log.w(Engine.TAG, "voice message: no access to all files or no model")
            return
        }
        val extras = notification.extras
        val group = extras.getCharSequence(Notification.EXTRA_CONVERSATION_TITLE)
        val sender = message.sender ?: extras.getCharSequence(Notification.EXTRA_TITLE)
        val who = when {
            group != null && sender != null && group.toString() != sender.toString() -> "$sender ($group)"
            else -> sender?.toString()
        }
        val chat = (group ?: sender)?.toString().orEmpty()
        // The file is written while WhatsApp downloads it, around the notification's time.
        val since = minOf(message.time, sbn.postTime) - SLACK_MILLIS
        worker.execute { transcribe(who, chat, sender?.toString().orEmpty(), since, notification.contentIntent) }
    }

    /** The newest message of a chat's notification (MessagingStyle), else its text. */
    private fun lastMessage(notification: Notification, posted: Long): Message {
        val extras = notification.extras
        val bundles: Array<Parcelable>? = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            extras.getParcelableArray(Notification.EXTRA_MESSAGES, Parcelable::class.java)
        } else {
            @Suppress("DEPRECATION")
            extras.getParcelableArray(Notification.EXTRA_MESSAGES)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val last = bundles?.let { Notification.MessagingStyle.Message.getMessagesFromBundleArray(it) }?.lastOrNull()
            if (last != null) return Message(last.text, last.senderPerson?.name, last.timestamp)
        } else {
            // Android 10: the message bundles' own keys ("text", "time", "sender").
            val last = bundles?.lastOrNull() as? Bundle
            if (last != null) return Message(last.getCharSequence("text"), last.getCharSequence("sender"), last.getLong("time"))
        }
        return Message(extras.getCharSequence(Notification.EXTRA_TEXT), null, posted)
    }

    /** On the worker: wait for WhatsApp's download, then decode and transcribe it. */
    private fun transcribe(who: String?, chat: String, sender: String, since: Long, open: PendingIntent?) {
        val file = waitForFile(since) ?: run {
            Log.w(Engine.TAG, "voice message: no new file in WhatsApp's folder")
            return
        }
        handle(this, file, who, chat, sender, open)
    }

    /** The new file once its size stopped changing (downloaded), or null after a while. */
    private fun waitForFile(since: Long): File? {
        val roots = VoiceNotes.FOLDERS.map { File(Environment.getExternalStorageDirectory(), it) }
        var candidate: File? = null
        var size = -1L
        repeat(WAIT_STEPS) {
            val found = synchronized(taken) { VoiceNotes.newFile(roots, since, taken) }
            if (found != null && found == candidate && found.length() == size) {
                synchronized(taken) {
                    taken.add(found.path)
                    while (taken.size > 200) taken.remove(taken.first())
                }
                return found
            }
            candidate = found
            size = found?.length() ?: -1L
            Thread.sleep(STEP_MILLIS)
        }
        return null
    }

    companion object {
        /**
         * A voice message's file to text: decode, transcribe, keep (recent transcripts, the chat's
         * strip) and show the notification. On a worker thread; the debug build's test trigger
         * (TestVoiceNoteReceiver) runs the same.
         */
        fun handle(context: Context, file: File, who: String?, chat: String, sender: String, open: PendingIntent?) {
            val decoded = try {
                AudioDecoder.decode(context, Uri.fromFile(file))
            } catch (e: Throwable) {
                Log.w(Engine.TAG, "voice message not decoded: ${Engine.describe(e)}")
                return
            }
            Engine.transcribe(context, decoded.samples, decoded.rate) { result ->
                result.onSuccess { text ->
                    if (text.isBlank()) return@onSuccess
                    val line = VoiceNotes.line(who, text)
                    val now = System.currentTimeMillis()
                    AppFiles(context).transcripts.add(Transcript(now, Transcript.Source.MESSAGE, line))
                    if (chat.isNotBlank()) AppFiles(context).voiceNotes.add(VoiceNote(now, chat, sender, text))
                    // An open chat's strip shows it at once.
                    FloatingMicService.instance?.voiceNotesChanged()
                    show(context, who, text, open)
                }.onFailure { Log.w(Engine.TAG, "voice message not transcribed: ${Engine.describe(it)}") }
            }
        }

        private fun show(context: Context, who: String?, text: String, open: PendingIntent?) {
            val manager = context.getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL, context.getString(R.string.voice_notes_channel), NotificationManager.IMPORTANCE_DEFAULT)
                    .apply {
                        // WhatsApp has rung already.
                        setSound(null, null)
                        enableVibration(false)
                    },
            )
            val id = ids.incrementAndGet()
            val copy = PendingIntent.getBroadcast(
                context,
                id,
                Intent(context, CopyReceiver::class.java).putExtra(CopyReceiver.TEXT, text).putExtra(CopyReceiver.ID, id),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            val tap = open ?: PendingIntent.getActivity(
                context,
                0,
                Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                PendingIntent.FLAG_IMMUTABLE,
            )
            val notification = Notification.Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.ic_bars)
                .setContentTitle("🎤 " + (who ?: context.getString(R.string.voice_notes_someone)))
                .setContentText(text)
                .setStyle(Notification.BigTextStyle().bigText(text))
                .setContentIntent(tap)
                .setAutoCancel(true)
                .addAction(Notification.Action.Builder(null, context.getString(R.string.copy), copy).build())
                .build()
            // The text itself is never logged; it is in the recent transcripts.
            runCatching { manager.notify(TAG_NOTIFICATION, id, notification) }
                .onFailure { Log.w(Engine.TAG, "transcript notification not shown: ${it.javaClass.simpleName}") }
        }

        private const val CHANNEL = "voice_notes"
        const val TAG_NOTIFICATION = "voice_note"
        private const val SLACK_MILLIS = 30_000L
        private const val STEP_MILLIS = 500L

        /** Up to a minute for WhatsApp to download the message. */
        private const val WAIT_STEPS = 120
        private val ids = AtomicInteger(1000)
    }
}

/** The transcript notification's Copy. */
class CopyReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val text = intent.getStringExtra(TEXT) ?: return
        Setup.copy(context, text)
        context.getSystemService(NotificationManager::class.java)
            .cancel(VoiceNoteListener.TAG_NOTIFICATION, intent.getIntExtra(ID, 0))
    }

    companion object {
        const val TEXT = "text"
        const val ID = "id"
    }
}
