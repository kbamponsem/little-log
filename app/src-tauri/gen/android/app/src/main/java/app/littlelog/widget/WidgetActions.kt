package app.littlelog.widget

/**
 * The six widget actions, shared by the widget, the callback and the launcher
 * shortcuts. The ids match `WidgetAction` in little-log-core.
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

    /** Everything the core knows how to apply. */
    val all = setOf(BREAST_LEFT, BREAST_RIGHT, BOTTLE, PEE, POOP, SLEEP_TOGGLE)

    fun isSupported(action: String): Boolean = action in all

    /** The blocks to draw, and the buttons to place under them. */
    val buttons: List<Pair<String, String>> = listOf(
        BREAST_LEFT to "Left",
        BREAST_RIGHT to "Right",
        BOTTLE to "Bottle",
        PEE to "Pee",
        POOP to "Poop",
        SLEEP_TOGGLE to "Sleep",
    )
}
