package app.littlelog.widget

import android.content.Context
import androidx.glance.appwidget.updateAll
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/** Re-render the widget(s), optionally after recomputing the state file. */
object WidgetUpdater {
    /** Ask Glance to redraw every instance of the widget. */
    fun update(context: Context) {
        val appContext = context.applicationContext
        CoroutineScope(Dispatchers.IO).launch {
            runCatching { LittleLogWidget().updateAll(appContext) }
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
                appContext.filesDir.absolutePath,
                WidgetDrainWorker.tzOffsetMinutes(),
            )
        }
        update(appContext)
    }
}
