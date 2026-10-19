package app.littlelog.widget

import android.content.Context
import java.io.File

/**
 * JNI bridge into `little-log-core` (see `src/widget.rs`).
 *
 * These are the only two entry points the widget side needs, and neither of
 * them touches the UI: a broadcast receiver queues a tap, a WorkManager worker
 * drains the queue into SQLite and rewrites the widget's state file.
 */
object WidgetBridge {
    init {
        // The same cdylib Tauri loads; `System.loadLibrary` is idempotent.
        System.loadLibrary("little_log_app")
    }

    /**
     * Apply every queued tap to the app's database and refresh the widget's
     * state file. Returns true when at least one entry was logged.
     */
    external fun nativeDrain(dataDir: String, tzOffsetMinutes: Int): Boolean

    /** Recompute the widget's state file from the database. */
    external fun nativeRefresh(dataDir: String, tzOffsetMinutes: Int)
}

/**
 * Where the app keeps its data.
 *
 * Tauri's app data directory is the package data directory — the *parent* of
 * `filesDir` — and the app writes `widget_root.txt` there on startup so this
 * process opens exactly the same SQLite file.
 *
 * Looking only in `filesDir` (as this used to) never found the marker, and the
 * fallback then pointed the whole widget at a second, empty database: taps
 * landed there, and the app never saw them.
 */
object WidgetPaths {
    private const val MARKER = "widget_root.txt"

    fun root(context: Context): File {
        for (directory in listOf(context.dataDir, context.filesDir)) {
            val value = runCatching { File(directory, MARKER).readText().trim() }.getOrNull()
            if (!value.isNullOrEmpty()) return File(value)
        }
        // Before the app has ever run: the directory it will use.
        return context.dataDir
    }

    fun stateFile(context: Context): File = File(root(context), "widget_state.json")

    /** The action a widget tap wants the UI to open; Rust reads and clears it. */
    fun pendingAction(context: Context): File = File(root(context), "pending_action.txt")
}
