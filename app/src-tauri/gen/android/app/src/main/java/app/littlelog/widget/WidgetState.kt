package app.littlelog.widget

import android.content.Context
import org.json.JSONObject

/**
 * What the widget shows. Written by Rust (`crates/core/src/pending.rs`) after
 * every drain, so the widget renders from the same numbers the app uses.
 */
data class WidgetState(
    /** Which child these numbers are about; blank before the first drain. */
    val babyName: String,
    val lastFeedAt: Long?,
    val sleepingSince: Long?,
    val feedsToday: Int,
    val diapersToday: Int,
) {
    /**
     * Whether a sleep is running — the Sleep button reads "Wake" when it is.
     *
     * The widget shows actions only, so this is the one field it renders. The
     * core still writes the rest, so a summary can come back without touching
     * the Rust side.
     */
    val isSleeping: Boolean get() = sleepingSince != null

    companion object {
        val empty = WidgetState(
            babyName = "",
            lastFeedAt = null,
            sleepingSince = null,
            feedsToday = 0,
            diapersToday = 0,
        )

        /** Read the state file; a missing or unreadable file means "nothing logged". */
        fun read(context: Context): WidgetState {
            val file = WidgetPaths.stateFile(context)
            val text = runCatching { file.readText() }.getOrNull() ?: return empty
            return runCatching {
                val json = JSONObject(text)
                WidgetState(
                    babyName = json.optString("baby_name", ""),
                    lastFeedAt = if (json.isNull("last_feed_at")) null else json.getLong("last_feed_at"),
                    sleepingSince = if (json.isNull("sleeping_since")) null else json.getLong("sleeping_since"),
                    feedsToday = json.optInt("feeds_today", 0),
                    diapersToday = json.optInt("diapers_today", 0),
                )
            }.getOrElse { empty }
        }
    }
}
