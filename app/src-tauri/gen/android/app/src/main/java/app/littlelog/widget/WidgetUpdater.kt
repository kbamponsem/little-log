package app.littlelog.widget

import android.content.Context
import android.util.Log
import androidx.glance.appwidget.updateAll
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
     * Redraw and wait for it — for a WorkManager worker, whose process can be
     * torn down the moment `doWork` returns and would cancel a detached update.
     */
    suspend fun updateAndWait(context: Context) {
        runCatching { LittleLogWidget().updateAll(context.applicationContext) }
            .onFailure { Log.w(TAG, "could not redraw the widget", it) }
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
