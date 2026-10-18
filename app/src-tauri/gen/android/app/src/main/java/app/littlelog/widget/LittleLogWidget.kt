package app.littlelog.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.color.ColorProvider

/**
 * The home-screen widget: "Last fed 1h 20m ago", "Sleeping 42m" and six
 * one-tap buttons.
 *
 * Each button runs [LogActionCallback] — a background callback, so a tap logs
 * the event without the app appearing.
 */
class LittleLogWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val state = WidgetState.read(context)
        provideContent { WidgetContent(state) }
    }
}

private val bg = ColorProvider(day = Color(0xFF4FAE7E), night = Color(0xFF0F2A1D))
private val card = ColorProvider(day = Color(0xE6FFFFFF), night = Color(0xFF173A29))
private val ink = ColorProvider(day = Color(0xFF0F2418), night = Color(0xFFD6F2E2))

private val titleStyle = TextStyle(color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
private val bodyStyle = TextStyle(color = ink, fontSize = 13.sp, fontWeight = FontWeight.Medium)
private val buttonStyle = TextStyle(color = ink, fontSize = 13.sp, fontWeight = FontWeight.Medium)

@Composable
private fun WidgetContent(state: WidgetState) {
    Column(
        modifier = GlanceModifier
            .fillMaxWidth()
            .background(bg)
            .padding(12.dp),
    ) {
        Text("Little Log", style = titleStyle)
        Text(state.countsLabel(), style = bodyStyle)
        Text(state.lastFedLabel(), style = bodyStyle)
        Text(state.sleepLabel(), style = bodyStyle)

        Spacer(GlanceModifier.height(8.dp))

        // Glance 1.2 has no weight modifier, so the buttons are a fixed size
        // that fits the 4x2 widget cell.
        Row {
            button(WidgetActions.BREAST_LEFT, "Left")
            button(WidgetActions.BREAST_RIGHT, "Right")
            button(WidgetActions.BOTTLE, "Bottle")
        }
        Spacer(GlanceModifier.height(6.dp))
        Row {
            button(WidgetActions.PEE, "Pee")
            button(WidgetActions.POOP, "Poop")
            button(WidgetActions.SLEEP_TOGGLE, if (state.isSleeping) "Wake" else "Sleep")
        }
    }
}

@Composable
private fun button(action: String, label: String) {
    Box(
        modifier = GlanceModifier
            .width(76.dp)
            .padding(end = 6.dp)
            .background(card)
            .padding(vertical = 10.dp)
            .clickable(
                actionRunCallback<LogActionCallback>(
                    actionParametersOf(LogActionCallback.ACTION_KEY to action),
                ),
            ),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, style = buttonStyle)
    }
}
