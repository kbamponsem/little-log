package app.littlelog.feed

/**
 * JNI bridge into `little-log-core` for the feeding timer
 * (see `app/src-tauri/src/widget.rs`).
 *
 * Both calls open the same SQLite file as the app. The stop deliberately
 * authors an event instead of writing the row, so a feed ended from the
 * notification replicates like one ended in the app.
 */
object FeedBridge {
    init {
        // The same cdylib Tauri loads; `System.loadLibrary` is idempotent.
        System.loadLibrary("little_log_app")
    }

    /** Epoch millis when the running feed started, or 0 when none is running. */
    external fun nativeRunningStart(dataDir: String): Long

    /** Stop the running feed. Returns true when a feed was running. */
    external fun nativeStop(dataDir: String): Boolean
}
