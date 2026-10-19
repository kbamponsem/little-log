import { defineComponent, LLComponent } from "../../core/component";
import { ui } from "../../core/ui-store";
import type { Toast } from "../../core/ui-store";
import "./ll-toast-host.css";

/** How long a message stays up. Long enough to read, short enough to ignore. */
const LIFETIME_MS = 5000;

/**
 * Transient messages, in one place.
 *
 * Anything can report news by calling `ui.toast(...)`, and the timing lives here
 * rather than at the call site — a message about something that happened should
 * not depend on whoever reported it still being on screen. The join flow is the
 * first user: it reports "waiting" in the sheet and the answer as a toast, and
 * the answer often arrives while the person is looking elsewhere.
 */
export class LlToastHost extends LLComponent {
  /** Per-toast timers, so a re-render does not restart the clock. */
  private timers = new Map<number, number>();

  protected template(): string {
    return `<div class="toasts" data-slot="toasts" role="status" aria-live="polite"></div>`;
  }

  protected subscribe(): void {
    this.watch(ui, () => this.sync());
    this.sync();
  }

  protected override disconnect(): void {
    for (const timer of this.timers.values()) window.clearTimeout(timer);
    this.timers.clear();
  }

  private sync(): void {
    const slot = this.querySelector<HTMLElement>('[data-slot="toasts"]');
    if (!slot) return;

    slot.replaceChildren(...ui.toasts.map((toast) => this.element(toast)));

    // Only start a clock for toasts that do not have one: `sync` runs on every
    // ui change, and re-arming would keep a message up for as long as anything
    // else keeps changing.
    for (const toast of ui.toasts) {
      if (this.timers.has(toast.id)) continue;
      this.timers.set(
        toast.id,
        window.setTimeout(() => {
          this.timers.delete(toast.id);
          ui.dismissToast(toast.id);
        }, LIFETIME_MS),
      );
    }
  }

  private element(toast: Toast): HTMLElement {
    const node = document.createElement("div");
    node.className = `toast ${toast.kind}`;
    node.dataset.toast = String(toast.id);
    node.textContent = toast.message;
    return node;
  }
}

defineComponent("ll-toast-host", LlToastHost);
