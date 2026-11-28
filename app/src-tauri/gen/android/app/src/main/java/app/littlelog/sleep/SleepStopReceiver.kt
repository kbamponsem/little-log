package app.littlelog.sleep

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import app.littlelog.timer.SessionTimers
import app.littlelog.widget.WidgetPaths
import app.littlelog.widget.WidgetUpdater

/**
 * Ends a sleep from the notification without opening the app.
 *
 * Same shape as [app.littlelog.feed.FeedStopReceiver]: the work runs off the
 * main thread through `goAsync`, the stop is authored through the core event
 * log so it replicates, and the widget is refreshed because the sleep state
 * changed. If the stop fails the notification is kept; the sleep is still
 * running, and the timer is the way back to it.
 */
class SleepStopReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_STOP) return

        val app = context.applicationContext
        val result = goAsync()
        Thread(
            {
                try {
                    val dataDir = WidgetPaths.root(app).absolutePath
                    val stopped = SleepBridge.nativeStop(dataDir)
                    SleepTimer.cancel(app)
                    // Remove the current scheduler's notification too, if this
                    // action came from a notification posted by an older build.
                    SessionTimers.sync(app)
                    WidgetUpdater.refreshAndUpdate(app)
                    Log.i(
                        TAG,
                        if (stopped) "sleep ended from the notification" else "no sleep was running",
                    )
                } catch (error: Throwable) {
                    Log.e(TAG, "could not end the sleep; keeping the timer", error)
                } finally {
                    result.finish()
                }
            },
            "sleep-stop",
        ).start()
    }

    companion object {
        private const val TAG = "SleepStopReceiver"
        const val ACTION_STOP = "app.littlelog.sleep.STOP"
    }
}
