package app.littlelog.timer

import android.Manifest
import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.drawable.Icon
import android.os.Build
import android.os.Bundle
import androidx.annotation.RequiresApi
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat

/**
 * One running activity shown by [TimerNotification].
 *
 * Everything that differs between feeding and sleeping lives here, so both
 * timers render through the same code and pick up the same Android 16 Live
 * Update treatment.
 */
data class TimerSpec(
    val notificationId: Int,
    val channelId: String,
    val channelName: String,
    val channelDescription: String,
    val title: String,
    /** Tap on the notification body: open the app on this activity's sheet. */
    val open: PendingIntent,
    /** Action shown beside the clock: Stop the feed, or open the wake-up sheet. */
    val stopLabel: String,    val stop: PendingIntent,
    val smallIcon: Int,
    val color: Int,
    /** Epoch millis when the activity started; the chronometer's origin. */
    val startedAt: Long,
    /** The open entry this notification represents; a stale stop cannot end a newer timer. */
    val entryId: String = "",
    /** Second row: the feed side, or a short hint to open the app. */
    val contentText: String = "Tap to open",
    /** Feed only: close this side's segment and open the other side. */
    val switchSideLabel: String? = null,
    val switchSide: PendingIntent? = null,
)

/**
 * The timer notification engine.
 *
 * Android 16's Live Updates promote an ongoing notification to the lock screen
 * and to a status-bar chip, and a chip can carry a running chronometer. When
 * the device supports that (API 36+) and allows this app to post promoted
 * notifications, the timer uses the platform builder. Otherwise it falls back
 * to a standard `NotificationCompat` ongoing notification, which works down to
 * Android 7.
 *
 * Both paths use the system chronometer from [TimerSpec.startedAt], so the app
 * never wakes up to tick the elapsed time.
 */
object TimerNotification {
    /** The pre-36.1 spelling of `Notification.Builder#setRequestPromotedOngoing`. */
    private const val EXTRA_REQUEST_PROMOTED_ONGOING = "android.requestPromotedOngoing"

    /**
     * Timer notifications live in this id range, one id per (child, kind).
     * `cancelExcept` uses it to find our own notifications after a process
     * restart, when no in-memory list of what was posted survives.
     *
     * 4300+ keeps clear of the fixed 4101/4102 ids older builds used.
     */
    const val ID_BASE = 4300
    const val ID_RANGE = 200

    /** Hidden extra tying a posted notification to the entry it rendered. */
    private const val EXTRA_ENTRY_ID = "app.littlelog.timer.ENTRY_ID"

    @SuppressLint("NewApi") // canPromote gates every API 36 call.
    fun show(context: Context, spec: TimerSpec) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            // The activity still works; there is just no timer on the lock screen.
            return
        }

        ensureChannel(context, spec)
        val notification = if (canPromote(context)) {
            liveUpdate(context, spec)
        } else {
            classic(context, spec)
        }
        NotificationManagerCompat.from(context).notify(spec.notificationId, notification)
    }

    fun cancel(context: Context, notificationId: Int) {
        NotificationManagerCompat.from(context).cancel(notificationId)
    }

    /**
     * Cancel [notificationId] only if it is still showing [entryId].
     *
     * Two open feeds for the same child can exist briefly (one started before
     * sync saw it, say). Stopping the older one must not remove the timer that
     * is now showing the newer entry.
     */
    fun stop(context: Context, notificationId: Int, entryId: String?) {
        if (entryId == null) {
            cancel(context, notificationId)
            return
        }
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val showing = runCatching {
            manager.activeNotifications.firstOrNull { it.id == notificationId }
                ?.notification?.extras?.getString(EXTRA_ENTRY_ID)
        }.getOrNull()
        if (showing == null || showing == entryId) {
            manager.cancel(notificationId)
        }
    }

    /**
     * Cancel every timer notification of ours that is not in [keep].
     *
     * Read from the system, not from remembered state, so a process restart or
     * a timer stopped on another device still removes the old notification.
     */
    fun cancelExcept(context: Context, keep: Set<Int>) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val active = runCatching { manager.activeNotifications }.getOrNull() ?: return
        for (notification in active) {
            if (notification.packageName != context.packageName) continue
            val id = notification.id
            // 4101/4102 were the fixed ids of the pre-timer-scheduler builds.
            val legacy = id == 4101 || id == 4102
            val ours = id >= ID_BASE && id < ID_BASE + ID_RANGE
            if (legacy || (ours && id !in keep)) manager.cancel(id)
        }
    }

    /**
     * Promotion exists from Android 16 (API 36), and the user can turn it off
     * for the app. `canPostPromotedNotifications` is the only reliable check;
     * on 36.0 the promotion request is simply ignored.
     */
    @SuppressLint("NewApi") // the SDK_INT guard below gates the API 36 call
    private fun canPromote(context: Context): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.BAKLAVA) return false
        return runCatching {
            context.getSystemService(NotificationManager::class.java)
                .canPostPromotedNotifications()
        }.getOrDefault(false)
    }

    /**
     * Android 16+: a promoted Live Update. Custom RemoteViews disqualify a
     * notification from promotion, so this path uses a platform style only;
     * the status-bar chip draws the chronometer from `setUsesChronometer`.
     */
    @RequiresApi(Build.VERSION_CODES.BAKLAVA)
    private fun liveUpdate(context: Context, spec: TimerSpec): Notification {
        val builder = Notification.Builder(context, spec.channelId)
            .setSmallIcon(spec.smallIcon)
            .setColor(spec.color)
            .setContentTitle(spec.title)
            .setContentText(spec.contentText)
            .setWhen(spec.startedAt)
            .setShowWhen(true)
            .setUsesChronometer(true)
            .setChronometerCountDown(false)
            .setOngoing(true)
            .setAutoCancel(false)
            .setOnlyAlertOnce(true)
            // The channel is IMPORTANCE_LOW and has no sound or vibration.
            .setCategory(Notification.CATEGORY_STOPWATCH)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setContentIntent(spec.open)
            .addAction(
                Notification.Action.Builder(
                    Icon.createWithResource(context, spec.smallIcon),
                    spec.stopLabel,
                    spec.stop,
                ).build(),
            )
            .setStyle(Notification.BigTextStyle().bigText(spec.contentText))
            // Request Live Update treatment. The extra is understood by every
            // API 36 build, including 36.0, where promotion is simply ignored.
            .addExtras(
                Bundle().apply {
                    putBoolean(EXTRA_REQUEST_PROMOTED_ONGOING, true)
                    putString(EXTRA_ENTRY_ID, spec.entryId)
                },
            )

        spec.switchSide?.let { switch ->
            builder.addAction(
                Notification.Action.Builder(
                    Icon.createWithResource(context, spec.smallIcon),
                    spec.switchSideLabel ?: "Switch side",
                    switch,
                ).build(),
            )
        }
        return builder.build()
    }

    /** Pre-Live Update: a standard ongoing notification with a system chronometer. */
    private fun classic(context: Context, spec: TimerSpec): Notification {
        val builder = NotificationCompat.Builder(context, spec.channelId)
            .setSmallIcon(spec.smallIcon)
            .setColor(spec.color)
            .setContentTitle(spec.title)
            .setContentText(spec.contentText)
            // The system draws and ticks the count-up from the start timestamp;
            // no RemoteViews and no app wake-up to refresh it.
            .setWhen(spec.startedAt)
            .setShowWhen(true)
            .setUsesChronometer(true)
            .setChronometerCountDown(false)
            .setOngoing(true)
            .setAutoCancel(false)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_STOPWATCH)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setContentIntent(spec.open)
            .addAction(0, spec.stopLabel, spec.stop)
            .addExtras(Bundle().apply { putString(EXTRA_ENTRY_ID, spec.entryId) })

        spec.switchSide?.let { switch ->
            builder.addAction(0, spec.switchSideLabel, switch)
        }
        return builder.build()
    }

    private fun ensureChannel(context: Context, spec: TimerSpec) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            spec.channelId,
            spec.channelName,
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = spec.channelDescription
            setShowBadge(false)
            // A running timer is glanceable, never an alert.
            setSound(null, null)
            enableVibration(false)
        }
        context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }
}
