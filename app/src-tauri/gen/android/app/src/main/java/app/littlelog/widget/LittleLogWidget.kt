package app.littlelog.widget

import android.content.Context
import android.content.Intent
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.action.Action
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.action.actionSendBroadcast
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.RowScope
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.color.ColorProvider
// `ColorProvider(day, night)` above is Glance's factory; the type it returns
// lives in `unit`, so it needs a name of its own here.
import androidx.glance.unit.ColorProvider as WidgetColor

import app.littlelog.MainActivity
import app.littlelog.R

/**
 * The home-screen widget: four one-tap actions.
 *
 * Feed and Diaper open the app carrying that action, and `MainActivity` hands
 * it to the matching sheet. Sleep and Wake Up stay on the home screen: a
 * broadcast reaches [WidgetActionReceiver] in the app process, which applies
 * the tap through the Rust core and redraws without opening anything.
 *
 * The buttons carry the app's category colours — feed is the butter yellow,
 * sleep the periwinkle, diaper the pink, wake-up the mood green — so the widget
 * reads at a glance before any label is. That is why the icons and text use the
 * dark `onTint` ink rather than the theme's: those pastels are light in both
 * themes, exactly as they are in the app. Everything else mirrors the app's
 * tokens: an airy light surface, and the dark green the app keeps for its dark
 * theme.
 */
class LittleLogWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        // Read *inside* the composition. A value captured out here is fixed for
        // the life of the session, so an update event would recompose the widget
        // with the numbers it had when the session started — which is how a tap
        // could be logged, stored, and still not appear until the app restarted.
        provideContent { WidgetContent(context, WidgetState.read(context)) }
    }
}

private val bg = ColorProvider(day = Color(0xFFEEF1EF), night = Color(0xFF0F2A1D))

// The app's category colours (see web/src/styles/tokens.css). Day and night are
// both pastels, so one dark ink reads on all of them.
private val feed = ColorProvider(day = Color(0xFFF7E9A6), night = Color(0xFFD9C46A))
private val sleepTint = ColorProvider(day = Color(0xFFB9C8F5), night = Color(0xFF8FA3E8))
private val diaper = ColorProvider(day = Color(0xFFF6B5C4), night = Color(0xFFE58FA5))
private val wake = ColorProvider(day = Color(0xFFB9E6C8), night = Color(0xFF7FC99C))
private val idle = ColorProvider(day = Color(0xFFDDE4E0), night = Color(0xFFA9BDB2))
private val onTint = ColorProvider(day = Color(0xFF0F2418), night = Color(0xFF0F2418))
private val ink = ColorProvider(day = Color(0xFF0F2418), night = Color(0xFFD6F2E2))

/** Big enough to read at a glance and to hit without looking. */
private val buttonStyle = TextStyle(color = onTint, fontSize = 13.sp, fontWeight = FontWeight.Medium)
private val titleStyle = TextStyle(color = ink, fontSize = 16.sp, fontWeight = FontWeight.Bold)

@Composable
private fun WidgetContent(context: Context, state: WidgetState) {
    Column(
        modifier = GlanceModifier
            .fillMaxWidth()
            .background(bg)
            .padding(8.dp)
            // Anywhere that is not a button opens the app, with no action.
            .clickable(actionStartActivity(Intent(context, MainActivity::class.java))),
    ) {
        Row(
            modifier = GlanceModifier.fillMaxWidth().padding(bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Image(
                provider = ImageProvider(R.drawable.ic_widget_logo),
                contentDescription = null, // the title beside it says it
                modifier = GlanceModifier.size(24.dp),
            )
            Spacer(GlanceModifier.width(8.dp))
            Text("Little Log", style = titleStyle)
        }
        // Equal weights keep the four tiles the same width; the spacers are the
        // only gap, so there is no extra space before the first or after the last.
        Row(GlanceModifier.fillMaxWidth()) {
            button(
                label = "Feed",
                icon = R.drawable.ic_widget_bottle,
                color = feed,
                action = openSheet(context, WidgetActions.FEED),
            )
            Spacer(GlanceModifier.width(8.dp))
            button(
                label = "Diaper",
                icon = R.drawable.ic_widget_pee,
                color = diaper,
                action = openSheet(context, WidgetActions.DIAPER),
            )
            Spacer(GlanceModifier.width(8.dp))
            // Only one of these applies at a time; the other stays visible but
            // inert, so the row never jumps around as sleep starts and ends.
            button(
                label = "Sleep",
                icon = R.drawable.ic_widget_sleep,
                color = sleepTint,
                action = if (state.isSleeping) null else logAction(context, WidgetActions.SLEEP),
            )
            Spacer(GlanceModifier.width(8.dp))
            button(
                label = "Wake Up",
                icon = R.drawable.ic_widget_wake,
                color = wake,
                action = if (state.isSleeping) logAction(context, WidgetActions.WAKE_UP) else null,
            )
        }
    }
}

/** Open the app on one of its sheets. */
private fun openSheet(context: Context, action: String) =
    actionStartActivity(
        Intent(context, MainActivity::class.java)
            .putExtra(WidgetActions.KEY, action),
    )

/**
 * Apply a one-tap action in the background and redraw the widget.
 *
 * This is an explicit broadcast, the same mechanism the feed notification's
 * Stop action uses, rather than Glance's `actionRunCallback`.
 */
private fun logAction(context: Context, action: String) =
    actionSendBroadcast(
        Intent(context, WidgetActionReceiver::class.java)
            .putExtra(WidgetActions.KEY, action),
    )

/**
 * One action: the app's icon over its label, in a tappable card filled with its
 * category colour. A null [action] draws the card as unavailable.
 */
@Composable
private fun RowScope.button(
    label: String,
    icon: Int,
    color: WidgetColor,
    action: Action?,
) {
    val base = GlanceModifier
        .defaultWeight()
        .background(if (action == null) idle else color)
        // After the background, so the colour is what gets rounded.
        .cornerRadius(20.dp)
        .padding(horizontal = 4.dp, vertical = 14.dp)

    Box(
        modifier = if (action == null) base else base.clickable(action),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = GlanceModifier.fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Image(
                provider = ImageProvider(icon),
                contentDescription = null, // the label below says it
                modifier = GlanceModifier.size(32.dp),
                colorFilter = ColorFilter.tint(onTint),
            )
            Spacer(GlanceModifier.height(6.dp))
            Text(label, style = buttonStyle)
        }
    }
}
