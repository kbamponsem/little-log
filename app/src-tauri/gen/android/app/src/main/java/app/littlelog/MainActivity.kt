package app.littlelog

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import app.littlelog.widget.WidgetActions
import app.littlelog.widget.WidgetPaths
import app.littlelog.widget.WidgetUpdater

class MainActivity : TauriActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        handOffWidgetAction(intent)
    }

    /**
     * A widget tap or launcher shortcut arriving while the app is already
     * running comes here instead of to `onCreate`, because the activity is
     * `singleTask`. Handling only `onCreate` meant the action was dropped
     * whenever the app was alive — the tap opened the app and did nothing else.
     */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handOffWidgetAction(intent)
    }

    override fun onStop() {
        super.onStop()
        // The app may have logged something; let the widget catch up.
        WidgetUpdater.refreshAndUpdate(this)
    }

    /**
     * A widget tap or launcher shortcut hands the app an action. The UI opens
     * that section's sheet and the entry is written when the user saves it, so
     * nothing is logged here — logging as well would record the same event
     * twice.
     */
    private fun handOffWidgetAction(intent: Intent?) {
        val action = intent?.getStringExtra(WidgetActions.KEY) ?: return
        intent.removeExtra(WidgetActions.KEY)
        if (!WidgetActions.isSupported(action)) return
        runCatching { WidgetPaths.pendingAction(this).writeText(action) }
    }
}
