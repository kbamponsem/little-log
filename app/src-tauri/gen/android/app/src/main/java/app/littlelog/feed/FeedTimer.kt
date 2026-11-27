package app.littlelog.feed

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.SystemClock
import android.widget.RemoteViews
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import app.littlelog.MainActivity
import app.littlelog.R
import app.littlelog.widget.WidgetActions
import app.littlelog.widget.WidgetPaths

/**
 * The running-feed notification.
 *
 * `setUsesChronometer(true)` makes the system draw and tick the elapsed time
 * from the feed's own start timestamp, so the count stays correct without us
 * waking up to update it. The Stop action is a broadcast that reaches the core
 * through [FeedBridge].
 */
object FeedTimer {
    private const val CHANNEL = "feeding"
    private const val NOTIFICATION_ID = 4101

    /**
     * Show the timer when a feed is running, remove it otherwise. Called when
     * the app goes to the background, which is when a parent stops looking at
     * the screen but the feed keeps going.
     */
    fun sync(context: Context) {
        val dataDir = WidgetPaths.root(context).absolutePath
        val startedAt = runCatching { FeedBridge.nativeRunningStart(dataDir) }.getOrDefault(0L)
        if (startedAt > 0L) show(context, startedAt) else cancel(context)
    }

    /**
     * [sync] off the main thread: it touches SQLite through JNI, and `onStop`
     * must not wait for disk work on a slow device.
     */
    fun syncAsync(context: Context) {
        val app = context.applicationContext
        Thread({ sync(app) }, "feed-timer").start()
    }

    fun cancel(context: Context) {
        NotificationManagerCompat.from(context).cancel(NOTIFICATION_ID)
    }

    private fun show(context: Context, startedAt: Long) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            // Feeding still works; there is just no timer on the lock screen.
            return
        }

        ensureChannel(context)

        val stop = PendingIntent.getBroadcast(
            context,
            0,
            Intent(context, FeedStopReceiver::class.java).setAction(FeedStopReceiver.ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val open = PendingIntent.getActivity(
            context,
            1,
            Intent(context, MainActivity::class.java)
                .putExtra(WidgetActions.KEY, WidgetActions.FEED),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        // A Chronometer counts from an elapsedRealtime base, not a wall clock,
        // so translate the feed's start into that timebase or it would show
        // decades of elapsed time.
        val base = SystemClock.elapsedRealtime() - (System.currentTimeMillis() - startedAt)
        val body = RemoteViews(context.packageName, R.layout.notification_feeding)
        body.setChronometer(R.id.feed_timer, base, null, true)

        val notification = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_feed_timer)
            .setColor(0xFF3E7C5A.toInt())
            .setContentTitle("Feeding")
            .setContentText("Tap to finish")
            .setCustomContentView(body)
            .setCustomBigContentView(body)
            .setStyle(NotificationCompat.DecoratedCustomViewStyle())
            // The body owns the clock; the header timestamp would repeat it.
            .setShowWhen(false)
            .setUsesChronometer(false)
            .setOngoing(true)
            .setAutoCancel(false)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_STOPWATCH)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setContentIntent(open)
            .addAction(0, "Stop", stop)
            .build()

        NotificationManagerCompat.from(context).notify(NOTIFICATION_ID, notification)
    }

    private fun ensureChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            CHANNEL,
            "Feeding timer",
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = "Shows a running feed while the app is in the background"
            setShowBadge(false)
        }
        context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }
}
