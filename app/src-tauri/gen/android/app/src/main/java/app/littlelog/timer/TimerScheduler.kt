package app.littlelog.timer

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import org.json.JSONArray
import org.json.JSONObject
import app.littlelog.MainActivity
import app.littlelog.R
import app.littlelog.widget.WidgetActions
import app.littlelog.widget.WidgetPaths

/**
 * One running feed or sleep, as little-log-core serialises it.
 *
 * The DB row is the source of truth; this is only the shape both the JNI path
 * and the Tauri plugin path render.
 */
data class TimerSnapshot(
    val kind: String,
    val entryId: String,
    val babyId: String?,
    val babyName: String,
    val startedAt: Long,
    val feedKind: String?,
) {
    val isFeed: Boolean get() = kind == "feed"

    /** "Feeding · Ada", or just "Sleeping" before a child has a name. */
    val title: String
        get() {
            val base = if (isFeed) "Feeding" else "Sleeping"
            val name = babyName.trim()
            return if (name.isEmpty()) base else "$base · $name"
        }

    /** The second row: which side, then a hint that tapping opens the app. */
    val contentText: String
        get() {
            val side = when (feedKind) {
                "breast_left" -> "Breast left"
                "breast_right" -> "Breast right"
                "bottle" -> "Bottle"
                "solids" -> "Solids"
                else -> null
            }
            return if (side == null) "Tap to open" else "$side · Tap to open"
        }

    /** Only a breast feed has a side to switch; bottles and sleep have none. */
    val canSwitchSide: Boolean
        get() = isFeed && (feedKind == "breast_left" || feedKind == "breast_right")

    companion object {
        fun from(json: JSONObject): TimerSnapshot? {
            val kind = json.optString("kind")
            if (kind != "feed" && kind != "sleep") return null
            val entryId = json.optString("entry_id")
            if (entryId.isEmpty()) return null
            return TimerSnapshot(
                kind = kind,
                entryId = entryId,
                babyId = if (json.isNull("baby_id")) null else json.optString("baby_id"),
                babyName = json.optString("baby_name"),
                startedAt = json.optLong("started_at"),
                feedKind = if (json.isNull("feed_kind")) null else json.optString("feed_kind"),
            )
        }

        fun listFrom(json: String): List<TimerSnapshot> {
            val array = runCatching { JSONArray(json) }.getOrNull() ?: return emptyList()
            return buildList {
                for (index in 0 until array.length()) {
                    val item = array.optJSONObject(index) ?: continue
                    from(item)?.let(::add)
                }
            }
        }
    }
}

/**
 * Turns running timers into notifications.
 *
 * One notification id per (child, kind), so a second feed for the same child
 * replaces the first rather than stacking, while siblings can each have their
 * own timer. Titles and clock origins come from the snapshot, never from
 * anything remembered here.
 */
object TimerScheduler {
    private const val CHANNEL_FEED = "feeding"
    private const val CHANNEL_SLEEP = "sleeping"

    fun id(kind: String, babyId: String?): Int {
        // A stable spread over the reserved range: 100 slots × 2 kinds.
        val slot = ((babyId ?: "").hashCode() and 0x7fffffff) % (TimerNotification.ID_RANGE / 2)
        return TimerNotification.ID_BASE + slot * 2 + if (kind == "sleep") 1 else 0
    }

    /** Replace the visible timers with the ones open in SQLite now. */
    fun sync(context: Context) {
        apply(context, TimerBridge.active(context))
    }

    fun apply(context: Context, timers: List<TimerSnapshot>) {
        val keep = HashSet<Int>(timers.size)
        for (timer in timers) {
            keep += id(timer.kind, timer.babyId)
            TimerNotification.show(context, spec(context, timer))
        }
        // A timer stopped on another device (or from the widget) is no longer
        // in the list; remove its notification without keeping a parallel copy.
        TimerNotification.cancelExcept(context, keep)
    }

    fun start(context: Context, timer: TimerSnapshot) {
        TimerNotification.show(context, spec(context, timer))
    }

    fun update(context: Context, timer: TimerSnapshot) {
        TimerNotification.show(context, spec(context, timer))
    }

    fun stop(context: Context, kind: String, babyId: String?, entryId: String?) {
        TimerNotification.stop(context, id(kind, babyId), entryId)
    }

    private fun spec(context: Context, timer: TimerSnapshot): TimerSpec {
        val notificationId = id(timer.kind, timer.babyId)
        val feed = timer.isFeed
        // Feed's Stop ends the session immediately. Sleep's "Wake up" opens the
        // sleep sheet instead, so the wake time and mood can be saved through the
        // same path as the in-app button.
        val stop = if (feed) {
            PendingIntent.getBroadcast(
                context,
                notificationId * 4,
                actionIntent(
                    context,
                    TimerActionReceiver.ACTION_STOP,
                    timer,
                ),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        } else {
            PendingIntent.getActivity(
                context,
                notificationId * 4,
                Intent(context, MainActivity::class.java)
                    .putExtra(WidgetActions.KEY, WidgetActions.SLEEP_TOGGLE),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        }
        // Tapping the body only opens the app; actions are what open a sheet.
        val open = PendingIntent.getActivity(
            context,
            notificationId * 4 + 1,
            Intent(context, MainActivity::class.java),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val switch = if (timer.canSwitchSide) {
            PendingIntent.getBroadcast(
                context,
                notificationId * 4 + 2,
                actionIntent(
                    context,
                    TimerActionReceiver.ACTION_SWITCH_SIDE,
                    timer,
                ),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        } else {
            null
        }

        return TimerSpec(
            notificationId = notificationId,
            channelId = if (feed) CHANNEL_FEED else CHANNEL_SLEEP,
            channelName = if (feed) "Feeding timer" else "Sleeping timer",
            channelDescription = if (feed) {
                "Shows a running feed while the app is in the background"
            } else {
                "Shows a running sleep while the app is in the background"
            },
            title = timer.title,
            contentText = timer.contentText,
            open = open,
            stopLabel = if (feed) "Stop" else "Wake up",
            stop = stop,
            switchSideLabel = if (switch != null) "Switch side" else null,
            switchSide = switch,
            smallIcon = if (feed) R.drawable.ic_feed_timer else R.drawable.ic_widget_sleep,
            color = if (feed) 0xFF3E7C5A.toInt() else 0xFF5B6FB0.toInt(),
            startedAt = timer.startedAt,
            entryId = timer.entryId,
        )
    }

    private fun actionIntent(
        context: Context,
        action: String,
        timer: TimerSnapshot,
    ): Intent = Intent(context, TimerActionReceiver::class.java)
        .setAction(action)
        .putExtra(TimerActionReceiver.EXTRA_KIND, timer.kind)
        .putExtra(TimerActionReceiver.EXTRA_ENTRY, timer.entryId)
        .putExtra(TimerActionReceiver.EXTRA_BABY, timer.babyId)
}

/**
 * JNI bridge into `little-log-core` for the timer notification
 * (see `app/src-tauri/src/widget.rs`).
 *
 * All three calls open the same SQLite file as the app. The stop and switch
 * author events, so they replicate exactly like the same action in the app.
 */
object TimerBridge {
    init {
        // The same cdylib Tauri loads; `System.loadLibrary` is idempotent.
        System.loadLibrary("little_log_app")
    }

    /** Every running feed/sleep, as JSON (the core's `ActiveTimer` list). */
    external fun nativeActiveTimers(dataDir: String): String

    /** End the open entry with this id. Returns true when it was running. */
    external fun nativeStopTimer(dataDir: String, entryId: String): Boolean

    /** Flip a breast feed to the other side. Returns true when it changed. */
    external fun nativeSwitchFeedSide(dataDir: String, entryId: String): Boolean

    fun active(context: Context): List<TimerSnapshot> {
        val dataDir = WidgetPaths.root(context).absolutePath
        return runCatching { TimerSnapshot.listFrom(nativeActiveTimers(dataDir)) }
            .getOrDefault(emptyList())
    }
}
