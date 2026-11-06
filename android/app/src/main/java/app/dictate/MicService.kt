package app.dictate

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.drawable.Icon
import android.os.Handler
import android.os.IBinder
import android.os.Looper

/**
 * The foreground service (type microphone) that runs while the floating button records and
 * transcribes: the notification with Stop, and the process kept alive. The accessibility service
 * starts it from the button's tap; its visible overlay makes that allowed (a visible system window
 * counts as being in the foreground). Recording has to wait until the service runs: Android
 * checks the microphone when recording starts, and an app that is neither on screen nor running
 * such a service gets silence (seen on Android 17: "App op 27 missing, silencing record").
 */
class MicService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            FloatingMicService.instance?.stopFromNotification()
            return START_NOT_STICKY
        }
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.mic_channel), NotificationManager.IMPORTANCE_LOW),
        )
        val stop = PendingIntent.getService(
            this,
            0,
            Intent(this, MicService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val notification = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_bars)
            .setContentTitle(getString(R.string.mic_notification))
            .setOngoing(true)
            .addAction(Notification.Action.Builder(null as Icon?, getString(R.string.mic_stop), stop).build())
            .build()
        // Always first: a service started with startForegroundService must call this in time.
        startForeground(ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
        running = this
        started()
        if (stopWanted) finish()
        return START_NOT_STICKY
    }

    private fun finish() {
        stopWanted = false
        running = null
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        running = null
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL = "recording"
        private const val ID = 1
        private const val ACTION_STOP = "app.dictate.STOP"
        private var running: MicService? = null
        private var stopWanted = false
        private val main = Handler(Looper.getMainLooper())
        private var onStarted: (() -> Unit)? = null
        private val giveUp = Runnable {
            android.util.Log.w(Engine.TAG, "microphone service did not start in time; recording anyway")
            started()
        }

        /**
         * Starts the service; [then] runs on the main thread once it is in the foreground (at once
         * when it already runs; when Android refuses or takes too long, anyway).
         */
        fun start(context: Context, then: () -> Unit) {
            stopWanted = false
            if (running != null) {
                then()
                return
            }
            onStarted = then
            try {
                context.startForegroundService(Intent(context, MicService::class.java))
                main.postDelayed(giveUp, 2000)
            } catch (e: IllegalStateException) {
                android.util.Log.w(Engine.TAG, "microphone service not started: $e")
                started()
            }
        }

        /** Forgets a [start] that has not called back yet. */
        fun cancelStart() {
            main.removeCallbacks(giveUp)
            onStarted = null
        }

        private fun started() {
            main.removeCallbacks(giveUp)
            val then = onStarted ?: return
            onStarted = null
            then()
        }

        /** Ends it, now or (not started yet) as soon as it has started. */
        fun stop() {
            cancelStart()
            val service = running
            if (service != null) service.finish() else stopWanted = true
        }
    }
}
