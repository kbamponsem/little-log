package app.littlelog.widget

import android.content.Context
import org.json.JSONObject

/**
 * What the widget shows. Written by Rust (`crates/core/src/pending.rs`) after
 * every drain, so the widget renders from the same numbers the app uses.
 */
data class WidgetState(
    val lastFeedAt: Long?,
    val sleepingSince: Long?,
    val feedsToday: Int,
    val diapersToday: Int,
) {
    val isSleeping: Boolean get() = sleepingSince != null

    /** "just now" / "12 min ago" / "1h 20m ago" — same wording as the app. */
    fun lastFedLabel(now: Long = System.currentTimeMillis()): String {
        val at = lastFeedAt ?: return "No feeds yet"
        val minutes = ((now - at) / 60_000).toInt()
        return when {
            minutes < 1 -> "Last fed just now"
            minutes < 60 -> "Last fed $minutes min ago"
            else -> "Last fed ${minutes / 60}h ${minutes % 60}m ago"
        }
    }

    /** "Sleeping 42m" / "Awake". */
    fun sleepLabel(now: Long = System.currentTimeMillis()): String {
        val since = sleepingSince ?: return "Awake"
        val minutes = ((now - since) / 60_000).toInt().coerceAtLeast(0)
        return "Sleeping ${minutes / 60}h ${minutes % 60}m"
    }

    fun countsLabel(): String = "Feeds $feedsToday · Diapers $diapersToday"

    companion object {
        val empty = WidgetState(lastFeedAt = null, sleepingSince = null, feedsToday = 0, diapersToday = 0)

        /** Read the state file; a missing or unreadable file means "nothing logged". */
        fun read(context: Context): WidgetState {
            val file = WidgetPaths.stateFile(context)
            val text = runCatching { file.readText() }.getOrNull() ?: return empty
            return runCatching {
                val json = JSONObject(text)
                WidgetState(
                    lastFeedAt = if (json.isNull("last_feed_at")) null else json.getLong("last_feed_at"),
                    sleepingSince = if (json.isNull("sleeping_since")) null else json.getLong("sleeping_since"),
                    feedsToday = json.optInt("feeds_today", 0),
                    diapersToday = json.optInt("diapers_today", 0),
                )
            }.getOrElse { empty }
        }
    }
}
