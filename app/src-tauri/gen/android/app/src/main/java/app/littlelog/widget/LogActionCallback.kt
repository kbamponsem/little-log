package app.littlelog.widget

import android.content.Context
import androidx.glance.GlanceId
import androidx.glance.action.ActionParameters
import androidx.glance.appwidget.action.ActionCallback

/**
 * A widget tap: apply the action and let Glance redraw.
 *
 * Glance already runs this in the app process through its broadcast trampoline
 * — no activity, no WebView, nothing on screen — so the Rust core is called
 * here rather than being left to a WorkManager worker. That matters for more
 * than latency: Glance repaints the widget when an action returns, and a redraw
 * requested from a worker arrives while that session is still queued (Glance
 * dedupes per widget), so the numbers stayed a tap behind until the app was
 * opened. Applying the tap first means the repaint reads the new state.
 *
 * The queue is still the safety net: if the direct drain fails, the action
 * stays queued and the worker retries it (ids make a repeat harmless).
 */
class LogActionCallback : ActionCallback {
    override suspend fun onAction(
        context: Context,
        glanceId: GlanceId,
        parameters: ActionParameters,
    ) {
        val action = parameters[ACTION_KEY] ?: return
        if (!WidgetActions.isSupported(action)) return

        PendingQueue.append(context, action)

        val drained = runCatching {
            WidgetBridge.nativeDrain(
                WidgetPaths.root(context).absolutePath,
                WidgetDrainWorker.tzOffsetMinutes(),
            )
        }
        if (drained.isFailure) {
            WidgetDrainWorker.schedule(context)
            return
        }
        // Redraw now that the entry is stored — awaited, because this coroutine
        // is the only thing keeping the request alive.
        WidgetUpdater.updateAndWait(context)
    }

    companion object {
        val ACTION_KEY = ActionParameters.Key<String>(WidgetActions.KEY)
    }
}
