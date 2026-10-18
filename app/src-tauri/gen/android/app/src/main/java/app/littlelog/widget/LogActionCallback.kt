package app.littlelog.widget

import android.content.Context
import androidx.glance.GlanceId
import androidx.glance.action.ActionParameters
import androidx.glance.appwidget.action.ActionCallback

/**
 * A widget tap: queue the action and ask for a drain.
 *
 * Glance runs this in the app process through its broadcast trampoline — no
 * activity, no WebView, nothing on screen. The WorkManager worker then applies
 * the tap in the Rust core.
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
        WidgetDrainWorker.schedule(context)
        // Reflect the new state as soon as the drain lands.
        WidgetUpdater.update(context)
    }

    companion object {
        val ACTION_KEY = ActionParameters.Key<String>(WidgetActions.KEY)
    }
}
