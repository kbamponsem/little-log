import { defineComponent, LLComponent } from "../../core/component";
import { ui } from "../../core/ui-store";
import { store } from "../../data/store";
import type { Role } from "../../data/model";
import "./ll-join-prompt.css";

/** How a role reads in the prompt's sentence. */
function rolePhrase(role: Role): string {
  switch (role) {
    case "owner":
      return "an owner";
    case "co_parent":
      return "a co-parent";
    case "caregiver":
      return "a caregiver";
  }
}

/**
 * The blocking prompt for a device waiting to be let into the log.
 *
 * Join requests are written to SQLite by the sync listener rather than through
 * the store, so `ll-app` polls for them and puts the first pending one in
 * `ui.joinPrompt`; this component only presents it. It is deliberately not a
 * bottom sheet: a request must not be lost behind whichever sheet is open, and
 * letting a device in is a decision that has to be made before it goes away.
 */
export class LlJoinPrompt extends LLComponent {
  private busy = false;
  private shownDevice: string | null = null;

  protected template(): string {
    return `
      <div class="join-prompt" data-role="prompt" hidden>
        <div class="join-prompt-card" role="alertdialog" aria-modal="true" aria-labelledby="join-prompt-title">
          <h2 id="join-prompt-title">Someone wants to join</h2>
          <p class="join-prompt-who" data-slot="who"></p>
          <p class="note">
            Accepting lets them read this child's log and add entries. Only
            accept a device you recognise.
          </p>
          <p class="err" data-slot="error" hidden></p>
          <button class="go" type="button" data-action="accept">Accept</button>
          <button class="del" type="button" data-action="decline">Decline</button>
        </div>
      </div>
    `;
  }

  protected mount(): void {
    this.sync();
  }

  protected subscribe(): void {
    this.watch(ui, () => this.sync());
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "accept") void this.resolve(true);
      if (action === "decline") void this.resolve(false);
    });
  }

  private sync(): void {
    const request = ui.joinPrompt;
    const wrapper = this.querySelector<HTMLElement>("[data-role='prompt']");
    if (!wrapper) return;
    wrapper.hidden = !request;
    if (!request) {
      this.shownDevice = null;
      return;
    }

    const who = this.querySelector<HTMLElement>("[data-slot='who']");
    if (who) {
      const child = store.baby.name || "this child";
      who.textContent = `Device ${request.device.slice(0, 8)}… wants to join ${child}'s log as ${rolePhrase(
        request.role,
      )}.`;
    }

    // Move focus once per request, not on every unrelated UI change.
    if (this.shownDevice !== request.device) {
      this.shownDevice = request.device;
      this.querySelector<HTMLButtonElement>("[data-action='accept']")?.focus();
    }
  }

  private async resolve(accept: boolean): Promise<void> {
    const request = ui.joinPrompt;
    if (!request || this.busy) return;

    this.busy = true;
    const error = this.querySelector<HTMLElement>("[data-slot='error']");
    if (error) {
      error.hidden = true;
      error.textContent = "";
    }

    try {
      await store.repo.resolveJoin(request.device, accept);
      ui.toast("success", accept ? "They can join now." : "Declined.");
      ui.setJoinPrompt(null);
    } catch (cause) {
      if (error) {
        error.hidden = false;
        error.textContent = cause instanceof Error ? cause.message : String(cause);
      }
    } finally {
      this.busy = false;
    }
  }
}

defineComponent("ll-join-prompt", LlJoinPrompt);
