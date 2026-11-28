package app.littlelog

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import app.littlelog.timer.SessionTimers
import app.littlelog.widget.WidgetActions
import app.littlelog.widget.WidgetPaths
import app.littlelog.widget.WidgetUpdater

class MainActivity : TauriActivity() {
    private val requestNotifications =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            // A timer may already be running; post it as soon as Android lets us.
            if (granted) SessionTimers.syncAsync(this)
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        handOffWidgetAction(intent)
        askForNotificationPermission()
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
        // The app may have logged something; let the widget catch up. Both of
        // these touch SQLite and Glance, so keep them off the main thread.
        val app = applicationContext
        Thread({ WidgetUpdater.refreshAndUpdate(app) }, "widget-refresh").start()
        // Leaving the app with a feed or sleep running is when the timer matters.
        SessionTimers.syncAsync(this)
    }

    /**
     * A widget tap or launcher shortcut hands the app an action. Feed, Diaper
     * and the launcher's detailed actions open that section's sheet and write
     * the entry when the user saves it, so nothing is logged here — logging as
     * well would record the same event twice. The widget's Sleep and Wake Up
     * log through the broadcast receiver and never reach this method.
     */
    private fun handOffWidgetAction(intent: Intent?) {
        val action = intent?.getStringExtra(WidgetActions.KEY) ?: return
        intent.removeExtra(WidgetActions.KEY)
        if (!WidgetActions.isSupported(action)) return
        runCatching { WidgetPaths.pendingAction(this).writeText(action) }
    }

    /**
     * Android 13+ needs an explicit grant before an app may post anything.
     * Denying it only costs the feed and sleep timers; the rest still works.
     */
    private fun askForNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        requestNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
}
