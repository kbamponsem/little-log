package app.littlelog.widget

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.util.Log
import androidx.glance.appwidget.GlanceAppWidgetManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/** Re-render the widget(s), optionally after recomputing the state file. */
object WidgetUpdater {
    private const val TAG = "LittleLogWidget"

    /**
     * Ask Glance to redraw every instance of the widget.
     *
     * Fire and forget: only safe from a caller that outlives the request (an
     * activity). Work that may end the process must use [updateAndWait].
     */
    fun update(context: Context) {
        val appContext = context.applicationContext
        CoroutineScope(Dispatchers.IO).launch { updateAndWait(appContext) }
    }

    /**
     * Redraw and wait for it — for a WorkManager worker or an action callback,
     * whose process can be torn down the moment the call returns.
     *
     * The instances are found through `AppWidgetManager`: Glance's own
     * `updateAll` looks them up through its cached provider map, and on this app
     * that map can be empty even while the widget is on screen, so the refresh
     * silently did nothing. Updating from inside an action callback also cannot
     * reuse the callback's own session, so this starts a fresh one per widget.
     */
    suspend fun updateAndWait(context: Context) {
        val appContext = context.applicationContext
        runCatching { updateEveryInstance(appContext) }
            .onFailure { Log.w(TAG, "could not redraw the widget", it) }
    }

    private suspend fun updateEveryInstance(context: Context) {
        val appWidgetManager = AppWidgetManager.getInstance(context)
        val component = ComponentName(context, LittleLogWidgetReceiver::class.java)
        val appWidgetIds = appWidgetManager.getAppWidgetIds(component)
        if (appWidgetIds.isEmpty()) return

        val glanceManager = GlanceAppWidgetManager(context)
        val widget = LittleLogWidget()
        for (appWidgetId in appWidgetIds) {
            widget.update(context, glanceManager.getGlanceIdBy(appWidgetId))
        }
    }

    /**
     * Recompute the state file from SQLite, then redraw. Used when the app
     * itself changed something (the widget must not go stale).
     */
    fun refreshAndUpdate(context: Context) {
        val appContext = context.applicationContext
        runCatching {
            WidgetBridge.nativeRefresh(
                WidgetPaths.root(appContext).absolutePath,
                WidgetDrainWorker.tzOffsetMinutes(),
            )
        }.onFailure { Log.w(TAG, "could not recompute the widget state", it) }
        update(appContext)
    }
}
