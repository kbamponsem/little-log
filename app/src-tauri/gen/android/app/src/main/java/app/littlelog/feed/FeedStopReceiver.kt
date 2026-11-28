package app.littlelog.feed

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import app.littlelog.timer.SessionTimers
import app.littlelog.widget.WidgetPaths
import app.littlelog.widget.WidgetUpdater

/**
 * Ends a feed from the notification without opening the app.
 *
 * The stop goes through [FeedBridge] into the Rust core, which authors an edit
 * event; the widget is refreshed afterwards because "last feed" has changed.
 *
 * The work runs off the main thread. `onReceive` has about ten seconds before
 * the system kills the receiver, and opening SQLite plus asking Glance to
 * redraw on the main thread was enough to miss a WorkManager job's callback and
 * show an ANR dialog right after the stop. If the stop fails the notification
 * is kept: the feed is still running, and the timer is the way back to it.
 */
class FeedStopReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_STOP) return

        val app = context.applicationContext
        val result = goAsync()
        Thread(
            {
                try {
                    val dataDir = WidgetPaths.root(app).absolutePath
                    val stopped = FeedBridge.nativeStop(dataDir)
                    FeedTimer.cancel(app)
                    // Remove the current scheduler's notification too, if this
                    // action came from a notification posted by an older build.
                    SessionTimers.sync(app)
                    WidgetUpdater.refreshAndUpdate(app)
                    Log.i(
                        TAG,
                        if (stopped) "feed stopped from the notification" else "no feed was running",
                    )
                } catch (error: Throwable) {
                    Log.e(TAG, "could not stop the feed; keeping the timer", error)
                } finally {
                    result.finish()
                }
            },
            "feed-stop",
        ).start()
    }

    companion object {
        private const val TAG = "FeedStopReceiver"
        const val ACTION_STOP = "app.littlelog.feed.STOP"
    }
}
