package app.littlelog

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import app.littlelog.widget.WidgetActions
import app.littlelog.widget.PendingQueue
import app.littlelog.widget.WidgetBridge
import app.littlelog.widget.WidgetDrainWorker
import app.littlelog.widget.WidgetUpdater

class MainActivity : TauriActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        logLauncherShortcut()
    }

    override fun onStop() {
        super.onStop()
        // The app may have logged something; let the widget catch up.
        WidgetUpdater.refreshAndUpdate(this)
    }

    /**
     * Launcher shortcuts reuse the widget's actions. They do open the app (that
     * is what a shortcut is), but the entry is written straight away — the UI
     * then shows it like any other log.
     */
    private fun logLauncherShortcut() {
        val action = intent?.getStringExtra(WidgetActions.KEY) ?: return
        intent.removeExtra(WidgetActions.KEY)
        PendingQueue.append(this, action)
        runCatching {
            WidgetBridge.nativeDrain(
                filesDir.absolutePath,
                WidgetDrainWorker.tzOffsetMinutes(),
            )
        }
    }
}
