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
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
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
 * The home-screen widget: six one-tap actions.
 *
 * A tap opens the app carrying that action, and `MainActivity` logs it straight
 * away (see `logLauncherShortcut`) — the same path the launcher shortcuts use,
 * so what the widget does and what a shortcut does cannot drift apart. Tapping
 * anywhere else opens the app with no action.
 *
 * The buttons carry the app's category colours — feed is the butter yellow,
 * sleep the periwinkle, diaper the pink — so the widget reads at a glance
 * before any label is. That is why the icons and text use the dark `onTint`
 * ink rather than the theme's: those pastels are light in both themes, exactly
 * as they are in the app. Everything else mirrors the app's tokens: an airy
 * light surface, and the dark green the app keeps for its dark theme.
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

// The app's three category colours (see web/src/styles/tokens.css). Day and
// night are both pastels, so one dark ink reads on all of them.
private val feed = ColorProvider(day = Color(0xFFF7E9A6), night = Color(0xFFD9C46A))
private val sleepTint = ColorProvider(day = Color(0xFFB9C8F5), night = Color(0xFF8FA3E8))
private val diaper = ColorProvider(day = Color(0xFFF6B5C4), night = Color(0xFFE58FA5))
private val onTint = ColorProvider(day = Color(0xFF0F2418), night = Color(0xFF0F2418))

/** Big enough to read at a glance and to hit without looking. */
private val buttonStyle = TextStyle(color = onTint, fontSize = 15.sp, fontWeight = FontWeight.Medium)

@Composable
private fun WidgetContent(context: Context, state: WidgetState) {
    Column(
        modifier = GlanceModifier
            .fillMaxWidth()
            .background(bg)
            .padding(12.dp)
            // Anywhere that is not a button opens the app, with no action.
            .clickable(actionStartActivity(Intent(context, MainActivity::class.java))),
    ) {
        // The buttons share the row evenly (`defaultWeight`), so they fill
        // whatever the launcher gives the widget instead of leaving a gap.
        Row(GlanceModifier.fillMaxWidth()) {
            button(context, WidgetActions.BREAST_LEFT, "Left", R.drawable.ic_widget_breast_left, feed, 0)
            button(context, WidgetActions.BREAST_RIGHT, "Right", R.drawable.ic_widget_breast_right, feed)
            button(context, WidgetActions.BOTTLE, "Bottle", R.drawable.ic_widget_bottle, feed)
        }
        Spacer(GlanceModifier.height(10.dp))
        Row(GlanceModifier.fillMaxWidth()) {
            button(context, WidgetActions.PEE, "Pee", R.drawable.ic_widget_pee, diaper, 0)
            button(context, WidgetActions.POOP, "Poop", R.drawable.ic_widget_poop, diaper)
            button(
                context,
                WidgetActions.SLEEP_TOGGLE,
                if (state.isSleeping) "Wake" else "Sleep",
                R.drawable.ic_widget_sleep,
                sleepTint,
            )
        }
    }
}

/**
 * One action: the app's icon over its label, in a tappable card filled with its
 * category colour.
 *
 * `gapStart` is the space to the previous card; the first in a row takes none,
 * so the row's outer margins stay equal.
 */
@Composable
private fun RowScope.button(
    context: Context,
    action: String,
    label: String,
    icon: Int,
    color: WidgetColor,
    gapStart: Int = 6,
) {
    Box(
        modifier = GlanceModifier
            .defaultWeight()
            .padding(start = gapStart.dp)
            .background(color)
            // After the background, so the colour is what gets rounded.
            .cornerRadius(20.dp)
            .padding(vertical = 14.dp)
            .clickable(
                actionStartActivity(
                    Intent(context, MainActivity::class.java)
                        .putExtra(WidgetActions.KEY, action),
                ),
            ),
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
            Spacer(GlanceModifier.height(4.dp))
            Text(label, style = buttonStyle)
        }
    }
}
