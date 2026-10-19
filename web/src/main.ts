/* Little Log — application entry point.
 *
 * Registers the component tree and boots. There is no login: boot resolves this
 * device's identity, creates the first baby if there is none, and loads the log.
 * `ll-app` owns everything after that.
 */

import "./styles/index.css";
// Registers every UI component (custom element + stylesheet)…
import "./components/ui";
// …then the app shell, which owns the screens, tabs and sheets.
import "./components/ll-app";

import { ui } from "./core/ui-store";
import type { SheetName } from "./core/ui-store";
import { store } from "./data/store";

/**
 * Which sheet each widget action opens.
 *
 * A tap on the widget logs nothing itself — it hands the app an action and
 * brings it forward, and the entry is written when the sheet is saved. That is
 * why these are the *creating* sheets and not the detail sheet: opening the
 * detail sheet would need an entry that does not exist yet.
 */
const WIDGET_SHEETS: Record<string, SheetName> = {
  breast_left: "feed",
  breast_right: "feed",
  bottle: "feed",
  pee: "diaper",
  poop: "diaper",
  sleep_toggle: "sleep",
};

/**
 * Open the sheet a widget tap asked for, if there is one waiting.
 *
 * Called after boot and whenever the app comes back to the foreground: a warm
 * tap arrives as `onNewIntent` and only makes the activity visible, so there is
 * no reload to hang this off.
 */
async function openWidgetSheet(): Promise<void> {
  let action: string | null = null;
  try {
    action = await store.repo.takeWidgetAction();
  } catch (error) {
    console.warn("Could not read the widget action", error);
    return;
  }
  const sheet = action ? WIDGET_SHEETS[action] : undefined;
  if (sheet) ui.openSheet({ name: sheet });
}

async function boot(): Promise<void> {
  try {
    await store.load();
  } catch (error) {
    // Show the app rather than sitting on the splash: an empty log the user can
    // still write to beats a screen that never appears.
    console.error("Could not load the log", error);
  }
  ui.markBooted();
  await openWidgetSheet();
}

/* The widget and the launcher shortcuts can also change the log while the app
 * sits in the background, and the WebView only reads it once, at boot. Re-read
 * on the way back so what they did is actually there to see. */
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState !== "visible") return;
  void store.refresh();
  void openWidgetSheet();
});

void boot();
