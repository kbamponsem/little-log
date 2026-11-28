package app.littlelog.timer

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * The Rust side's handle on the timer notifications.
 *
 * `start_timer`, `update_timer` and `stop_timer` mirror the Rust helpers in
 * `app/src-tauri/src/timer_notification.rs`. They carry the values the core
 * derived from the open entry; the plugin stores none of them. `syncTimers`
 * replaces the whole visible set, which is what makes a timer stopped on
 * another device disappear here as well.
 */
@InvokeArg
class TimerArgs {
    lateinit var kind: String
    var entryId: String? = null
    var babyId: String? = null
    var babyName: String? = null
    var startedAt: Long = 0
    var feedKind: String? = null

    fun snapshot(): TimerSnapshot = TimerSnapshot(
        kind = kind,
        entryId = entryId.orEmpty(),
        babyId = babyId,
        babyName = babyName.orEmpty(),
        startedAt = startedAt,
        feedKind = feedKind,
    )
}

@InvokeArg
class StopTimerArgs {
    lateinit var kind: String
    var babyId: String? = null
    var entryId: String? = null
}

@InvokeArg
class SyncTimersArgs {
    lateinit var timers: Array<TimerArgs>
}

@TauriPlugin
class TimerPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun startTimer(invoke: Invoke) {
        try {
            TimerScheduler.start(activity, invoke.parseArgs(TimerArgs::class.java).snapshot())
            invoke.resolve()
        } catch (error: Exception) {
            invoke.reject(error.message ?: "could not start the timer")
        }
    }

    @Command
    fun updateTimer(invoke: Invoke) {
        try {
            TimerScheduler.update(activity, invoke.parseArgs(TimerArgs::class.java).snapshot())
            invoke.resolve()
        } catch (error: Exception) {
            invoke.reject(error.message ?: "could not update the timer")
        }
    }

    @Command
    fun stopTimer(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(StopTimerArgs::class.java)
            TimerScheduler.stop(activity, args.kind, args.babyId, args.entryId)
            invoke.resolve()
        } catch (error: Exception) {
            invoke.reject(error.message ?: "could not stop the timer")
        }
    }

    @Command
    fun syncTimers(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SyncTimersArgs::class.java)
            TimerScheduler.apply(activity, args.timers.map { it.snapshot() })
            invoke.resolve()
        } catch (error: Exception) {
            invoke.reject(error.message ?: "could not sync the timers")
        }
    }
}
