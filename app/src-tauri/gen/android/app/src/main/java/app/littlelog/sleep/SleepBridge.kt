package app.littlelog.sleep

/**
 * JNI bridge into `little-log-core` for the sleeping timer
 * (see `app/src-tauri/src/widget.rs`).
 *
 * Both calls open the same SQLite file as the app. The wake deliberately
 * authors an event instead of writing the row, so a sleep ended from the
 * notification replicates like one ended in the app.
 */
object SleepBridge {
    init {
        // The same cdylib Tauri loads; `System.loadLibrary` is idempotent.
        System.loadLibrary("little_log_app")
    }

    /** Epoch millis when the running sleep started, or 0 when none is running. */
    external fun nativeRunningStart(dataDir: String): Long

    /** Wake the baby up. Returns true when a sleep was running. */
    external fun nativeStop(dataDir: String): Boolean
}
