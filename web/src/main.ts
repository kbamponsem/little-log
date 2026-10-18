/* Little Log — application entry point.
 *
 * Registers the component tree, wires the storage adapter's 401 hook, and
 * decides on boot whether to show the app or the sign-in screen.
 */

import "./styles/index.css";
// Registers every UI component (custom element + stylesheet)…
import "./components/ui";
// …then the app shell, which owns the sign-in screen, the tabs and the sheets.
import "./components/ll-app";

import { readInvite } from "./components/screens/ll-auth";
import { ui } from "./core/ui-store";
import { repo, store } from "./data/store";

/** `?invite=<token>` from the URL, if any. */
const invite = readInvite();

/** Any rejected request drops the app back to the sign-in screen. */
repo.setUnauthorizedHandler(() => {
  if (!ui.auth) ui.showAuth({ mode: "login", invite: null });
});

async function boot(): Promise<void> {
  let session = null;
  try {
    session = await repo.me();
  } catch {
    session = null;
  }

  if (session) {
    store.adoptSession(session);
    await store.refresh();
  } else {
    // Not signed in: the sign-in screen is the front door.
    ui.showAuth({ mode: "login", invite });
  }

  ui.markBooted();
}

void boot();
