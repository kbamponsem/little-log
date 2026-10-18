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
    external fun nativeDrain(filesDir: String, tzOffsetMinutes: Int): Boolean

    /** Recompute the widget's state file from the database. */
    external fun nativeRefresh(filesDir: String, tzOffsetMinutes: Int)
}

/**
 * Where the app keeps its data.
 *
 * The app writes `widget_root.txt` on startup so this process opens exactly the
 * same SQLite file; before the app has ever run we fall back to `filesDir`.
 */
object WidgetPaths {
    private const val MARKER = "widget_root.txt"

    fun root(context: Context): File {
        val marker = File(context.filesDir, MARKER)
        val value = runCatching { marker.readText().trim() }.getOrNull()
        return if (!value.isNullOrEmpty()) File(value) else context.filesDir
    }

    fun stateFile(context: Context): File = File(root(context), "widget_state.json")
}
