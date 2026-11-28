package app.littlelog.timer

import android.content.Context

/**
 * Keeps the running-activity notifications in step with the database.
 *
 * This is the headless path: the app calls it when it goes to the background,
 * the widget's action receiver calls it after applying a tap, and the timer's
 * own action receiver calls it after a stop or a side switch. The Tauri plugin
 * (`startTimer`/`updateTimer`/`stopTimer`/`syncTimers`) uses the same renderer
 * when Rust knows what changed.
 */
object SessionTimers {
    fun sync(context: Context) {
        TimerScheduler.sync(context)
    }

    /** [sync] off the main thread; it touches SQLite through JNI. */
    fun syncAsync(context: Context) {
        val app = context.applicationContext
        Thread({ sync(app) }, "session-timers").start()
    }
}
