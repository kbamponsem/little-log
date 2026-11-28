package app.littlelog.timer

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import app.littlelog.widget.WidgetPaths
import app.littlelog.widget.WidgetUpdater

/**
 * Stop or switch-side from the timer notification.
 *
 * The work runs off the main thread through `goAsync`, and both actions go
 * through the Rust core as events, so a tap here behaves exactly like the same
 * action in the app: it validates, sets the timestamps and replicates.
 *
 * After the write the whole set is re-derived from the database. The
 * notification therefore cannot get out of step with the entry: a stop removes
 * it, a switch re-renders the side, and an entry already stopped elsewhere just
 * cancels the stale notification.
 */
class TimerActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action ?: return
        if (action != ACTION_STOP && action != ACTION_SWITCH_SIDE) return
        val entryId = intent.getStringExtra(EXTRA_ENTRY) ?: return

        val app = context.applicationContext
        val result = goAsync()
        Thread(
            {
                try {
                    val dataDir = WidgetPaths.root(app).absolutePath
                    if (action == ACTION_STOP) {
                        TimerBridge.nativeStopTimer(dataDir, entryId)
                    } else {
                        TimerBridge.nativeSwitchFeedSide(dataDir, entryId)
                    }
                    TimerScheduler.sync(app)
                    // A stop changed "last feed" / "sleeping"; the widget must
                    // not wait for the app to be opened.
                    WidgetUpdater.refreshAndUpdate(app)
                } catch (error: Throwable) {
                    Log.e(TAG, "could not apply the timer action", error)
                } finally {
                    result.finish()
                }
            },
            "timer-action",
        ).start()
    }

    companion object {
        private const val TAG = "TimerActionReceiver"

        const val ACTION_STOP = "app.littlelog.timer.STOP"
        const val ACTION_SWITCH_SIDE = "app.littlelog.timer.SWITCH_SIDE"

        const val EXTRA_KIND = "app.littlelog.timer.KIND"
        const val EXTRA_ENTRY = "app.littlelog.timer.ENTRY"
        const val EXTRA_BABY = "app.littlelog.timer.BABY"
    }
}
