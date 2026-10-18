import { repo, store } from "../data/store";
import { ui } from "./ui-store";
import { LLComponent } from "./component";
import { errorMessage, fieldValue, setError } from "./form";

export type { FieldLike } from "./form";

/**
 * Base class for bottom-sheet components.
 *
 * The sheet host (`ll-sheet-host`) mounts one of these inside the sliding
 * `<section class="sheet">` container and reads the three host-facing
 * properties below. Sheets are `display: contents` (see ll-sheet-host.css),
 * so `template()` returns the sheet body: heading, groups, fields, actions.
 *
 * Sheets close themselves with `close()` (which clears the UI store) and
 * report failures through `error()` — both keep the host in charge.
 */
export abstract class LLSheet extends LLComponent {
  /** false → scrim taps and Escape will not close this sheet (auth). */
  dismissible = true;

  /** Focused when the sheet opens. */
  autofocusSelector = "input,textarea";

  /** Called by the host once the slide-out animation has finished. */
  onSheetClosed?(): void;

  /** The app-wide stores, for convenience in subclasses. */
  protected get store() {
    return store;
  }

  protected get repo() {
    return repo;
  }

  /** Ask the host to close this sheet. */
  protected close(): void {
    ui.closeSheet();
  }

  /** Read a named `ll-field` value (trimmed). */
  protected field(name: string): string {
    return fieldValue(this, name);
  }

  /** Show, or clear, the inline error line. */
  protected error(message: string | null): void {
    setError(this, message);
  }

  /** Turn any thrown value into a readable string. */
  protected message(error: unknown): string {
    return errorMessage(error);
  }
}
