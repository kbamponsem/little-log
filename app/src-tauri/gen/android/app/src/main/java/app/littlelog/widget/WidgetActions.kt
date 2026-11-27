package app.littlelog.widget

/**
 * The widget and launcher-shortcut actions. The ids match `WidgetAction` in
 * little-log-core, except `feed` and `diaper`, which only open a screen.
 */
object WidgetActions {
    /** Extra/parameter name carrying the action id. */
    const val KEY = "app.littlelog.widget.ACTION"

    const val BREAST_LEFT = "breast_left"
    const val BREAST_RIGHT = "breast_right"
    const val BOTTLE = "bottle"
    const val PEE = "pee"
    const val POOP = "poop"
    const val SLEEP_TOGGLE = "sleep_toggle"

    /** Opens the feeding sheet; the widget's Feed button and the notification. */
    const val FEED = "feed"

    /** Opens the diaper sheet; the widget's Diaper button. */
    const val DIAPER = "diaper"

    /** Starts a sleep in the background; a tap while asleep changes nothing. */
    const val SLEEP = "sleep"

    /** Ends the running sleep in the background; a tap while awake does nothing. */
    const val WAKE_UP = "wake_up"

    /** Everything the app accepts, whether it opens a sheet or logs directly. */
    val all = setOf(
        BREAST_LEFT,
        BREAST_RIGHT,
        BOTTLE,
        PEE,
        POOP,
        SLEEP_TOGGLE,
        FEED,
        DIAPER,
        SLEEP,
        WAKE_UP,
    )

    fun isSupported(action: String): Boolean = action in all

    /** The four widget buttons, in the order they are drawn. */
    val buttons: List<Pair<String, String>> = listOf(
        FEED to "Feed",
        DIAPER to "Diaper",
        SLEEP to "Sleep",
        WAKE_UP to "Wake Up",
    )
}
