package app.littlelog.widget

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import app.littlelog.timer.SessionTimers
import kotlinx.coroutines.runBlocking

/**
 * A widget tap: apply the action and redraw the widget.
 *
 * This is the same delivery path as the feed notification's Stop action: an
 * explicit broadcast to this receiver. Glance's own `ActionCallback` worked for
 * the tap itself, but on this app the callback never reliably ran, so Sleep and
 * Wake Up silently did nothing while Feed and Diaper (which launch an activity)
 * still worked. A manifest receiver plus `goAsync` is the path that already
 * proved reliable for the notification.
 *
 * The receiver body runs on its own thread because `onReceive` may not block
 * the main thread with JNI, SQLite and a Glance redraw.
 */
class WidgetActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.getStringExtra(WidgetActions.KEY) ?: return
        if (!WidgetActions.isSupported(action)) return

        val app = context.applicationContext
        val pending = goAsync()
        Thread(
            {
                try {
                    PendingQueue.append(app, action)
                    val drained = runCatching {
                        WidgetBridge.nativeDrain(
                            WidgetPaths.root(app).absolutePath,
                            WidgetDrainWorker.tzOffsetMinutes(),
                        )
                    }
                    if (drained.isFailure) {
                        WidgetDrainWorker.schedule(app)
                    }
                } catch (error: Throwable) {
                    Log.w(TAG, "could not apply the widget action; the worker will retry", error)
                    WidgetDrainWorker.schedule(app)
                } finally {
                    // A fresh session per widget: the tapped card has to
                    // recompose with the new sleep state.
                    runBlocking { WidgetUpdater.updateAndWait(app) }
                    // A Sleep tap posts the running-sleep timer; Wake Up
                    // removes it. Doing it here means the timer also appears
                    // when the app itself is not open.
                    runCatching { SessionTimers.sync(app) }
                        .onFailure { Log.w(TAG, "could not sync the running timers", it) }
                    pending.finish()
                }
            },
            "widget-action",
        ).start()
    }

    companion object {
        private const val TAG = "WidgetActionReceiver"
    }
}
