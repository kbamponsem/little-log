package app.littlelog.sleep

import android.content.Context
import app.littlelog.timer.SessionTimers
import app.littlelog.timer.TimerNotification

/**
 * Compatibility entry point for the sleep timer.
 *
 * The running timers are now rendered by [SessionTimers], one notification per
 * child and kind. This object stays so a notification posted by an older build
 * can still be cancelled, and so older lifecycle call sites keep working.
 */
object SleepTimer {
    private const val LEGACY_NOTIFICATION_ID = 4102

    fun sync(context: Context) {
        SessionTimers.sync(context)
    }

    fun syncAsync(context: Context) {
        SessionTimers.syncAsync(context)
    }

    fun cancel(context: Context) {
        TimerNotification.cancel(context, LEGACY_NOTIFICATION_ID)
    }
}
