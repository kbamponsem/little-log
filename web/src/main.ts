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
import { store } from "./data/store";

async function boot(): Promise<void> {
  try {
    await store.load();
  } catch (error) {
    // Show the app rather than sitting on the splash: an empty log the user can
    // still write to beats a screen that never appears.
    console.error("Could not load the log", error);
  }
  ui.markBooted();
}

void boot();
