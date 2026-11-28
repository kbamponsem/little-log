package app.littlelog.timer

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * Restore running-timer notifications after a reboot.
 *
 * Android drops every notification when the device restarts, but the open
 * entries are still in SQLite. This receiver re-derives them without opening
 * the app, so a nap that was running when the phone rebooted is visible again.
 *
 * If the user has not granted notification permission, [TimerNotification]
 * simply declines to post; the entries are untouched.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED &&
            intent.action != Intent.ACTION_MY_PACKAGE_REPLACED
        ) {
            return
        }

        val app = context.applicationContext
        val result = goAsync()
        Thread(
            {
                try {
                    SessionTimers.sync(app)
                } catch (error: Throwable) {
                    Log.w(TAG, "could not restore the timers after boot", error)
                } finally {
                    result.finish()
                }
            },
            "timer-boot",
        ).start()
    }

    companion object {
        private const val TAG = "TimerBootReceiver"
    }
}
