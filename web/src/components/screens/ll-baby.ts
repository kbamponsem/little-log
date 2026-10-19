import { defineComponent, LLComponent } from "../../core/component";
import { copyText } from "../../core/clipboard";
import { ui } from "../../core/ui-store";
import type { InviteRole } from "../../core/ui-store";
import type { Baby, Caregiver, JoinRequest } from "../../data/model";
import { INVITE_ROLES } from "../../data/model";
import { store } from "../../data/store";
import { esc } from "../../format";
import { LlCaregiverRow } from "../ui/ll-caregiver-row";
import type { LlChipGroup } from "../ui/ll-chip-group";
import "./ll-baby.css";

/** The current child's profile, the household's caregivers, and data controls. */
export class LlBaby extends LLComponent {
  private caregivers: Caregiver[] | null = null;
  /** Devices asking to be let in; refreshed with the caregivers. */
  private joins: JoinRequest[] | null = null;
  private saveTimer = 0;
  private deleteArmed = false;
  private joinPoll = 0;

  protected template(): string {
    const baby = store.baby;
    return `
      <h1>${esc(baby.name || "Baby")}</h1>
      <ll-card class="card">
        <ll-field name="name" label="Name" flush value="${esc(baby.name)}"></ll-field>
        <ll-field name="born" label="Birthday" type="date" value="${esc(baby.born)}"></ll-field>
      </ll-card>
      <ll-card class="card" heading="Children">
        <div data-slot="children"></div>
        <div class="grp">Add another baby</div>
        <ll-field name="new-name" label="Name" flush placeholder="Name"></ll-field>
        <ll-field name="new-born" label="Birthday" type="date"></ll-field>
        <button class="chip add-baby-button" type="button" data-action="add-baby">Add baby</button>
        <p class="note baby-error" data-slot="children-error" hidden></p>
        <p class="note">
          Everything you log is grouped around the child you are looking at, and
          each child keeps its own feeds, sleeps and diapers.
        </p>
      </ll-card>
      <ll-card class="card" heading="Caregivers">
        <div data-slot="joins"></div>
        <div data-slot="caregivers"></div>
        <div class="grp">Invite someone</div>
        <ll-chip-group data-role="invite-role"></ll-chip-group>
        <button class="chip invite-button" type="button" data-action="invite">Show invite code</button>
        <p class="note">Co-parents can edit everything; caregivers can view and add entries. The code carries this child's key, so show it only to someone you trust.</p>
        <div class="grp">Or join someone else's</div>
        <button class="chip join-button" type="button" data-action="join">Join with a code</button>
      </ll-card>
      <ll-card class="card" heading="Your data">
        <div class="chips">
          <button class="chip" type="button" data-action="export">Copy as CSV</button>
          <button class="chip delete-button" type="button" data-action="delete-all">Delete everything</button>
        </div>
        <p class="note">Everything is stored on this device. Nothing is sent anywhere.</p>
      </ll-card>
    `;
  }

  protected mount(): void {
    this.querySelector<LlChipGroup>('[data-role="invite-role"]')?.configure({
      options: INVITE_ROLES,
      value: ui.inviteRole,
      toggle: false,
    });
    this.renderChildren();
    this.renderCaregivers();
    this.renderJoins();
  }

  protected subscribe(): void {
    this.watch(store);
    void this.loadCaregivers();

    // Join requests are written to the database by the sync listener, not
    // through the store, so nothing here would notice one arriving. While this
    // screen is the one on show, re-read them so a waiting device can actually
    // be let in.
    this.joinPoll = window.setInterval(() => {
      if (!this.hidden && ui.tab === "baby") void this.refreshJoins();
    }, 3000);

    this.addEventListener("input", (event) => {
      const control = event.target as HTMLInputElement;
      // Only the child being looked at: the "add another" fields are prefixed
      // so typing a new baby's name cannot rename the current one.
      if (control.name === "name") this.saveSoon({ name: control.value });
      if (control.name === "born") this.saveSoon({ born: control.value });
    });

    this.addEventListener("click", (event) => {
      const target = event.target as HTMLElement;
      const action = target.closest<HTMLElement>("[data-action]")?.dataset.action;
      if (action === "invite") ui.openSheet({ name: "invite" });
      if (action === "join") ui.openSheet({ name: "join" });
      if (action === "export") void this.export(event);
      if (action === "delete-all") void this.deleteAll(event);
      if (action === "add-baby") void this.addBaby();
      const chip = target.closest<HTMLElement>("[data-baby]");
      if (chip?.dataset.baby) void store.selectBaby(chip.dataset.baby);
      const join = target.closest<HTMLElement>("[data-join]");
      if (join?.dataset.join) void this.resolveJoin(join.dataset.join, join.dataset.accept === "true");
    });

    this.addEventListener("ll-change", (event) => {
      const role = (event.target as HTMLElement).dataset.role;
      const value = (event as CustomEvent<{ value: string | null }>).detail.value;
      if (role === "invite-role" && value) ui.setInviteRole(value as InviteRole);
    });
  }

  protected override disconnect(): void {
    window.clearInterval(this.joinPoll);
    super.disconnect();
  }

  /**
   * The household's children, with the one this device is looking at marked.
   * A one-child household sees a single row, which is the point: the list is
   * where a second child is added.
   */
  private renderChildren(): void {
    const slot = this.querySelector<HTMLElement>('[data-slot="children"]');
    if (!slot) return;

    slot.replaceChildren(
      ...store.babies.map((baby) => {
        const row = document.createElement("div");
        row.className = "baby-child";

        const name = document.createElement("span");
        name.className = "baby-child-name";
        name.textContent = baby.name || "Baby";
        row.append(name);

        if (baby.id === store.baby.id) {
          const mark = document.createElement("span");
          mark.className = "baby-child-current";
          mark.textContent = "Looking at";
          row.append(mark);
        } else {
          const button = document.createElement("button");
          button.type = "button";
          button.className = "chip";
          button.dataset.baby = baby.id ?? "";
          button.textContent = "Look at";
          row.append(button);
        }
        return row;
      }),
    );
  }

  private async addBaby(): Promise<void> {
    const field = this.querySelector<HTMLInputElement>("ll-field[name='new-name'] input");
    const name = field?.value.trim() ?? "";
    if (!name) return;

    const born =
      this.querySelector<HTMLInputElement>("ll-field[name='new-born'] input")?.value ?? "";
    const error = this.querySelector<HTMLElement>('[data-slot="children-error"]');
    if (error) {
      error.hidden = true;
      error.textContent = "";
    }

    try {
      // Adding makes the new child the one being looked at, and the store
      // change re-renders this screen, which clears the form.
      await store.addBaby(name, born);
    } catch (cause) {
      if (error) {
        error.hidden = false;
        error.textContent = `Could not add ${name}: ${
          cause instanceof Error ? cause.message : cause
        }`;
      }
    }
  }

  private saveSoon(patch: Partial<Baby>): void {
    window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => {
      void store.saveBaby(patch).catch(() => undefined);
    }, 400);
  }

  private async loadCaregivers(): Promise<void> {
    try {
      this.caregivers = await store.repo.caregivers();
    } catch {
      this.caregivers = [];
    }
    this.renderCaregivers();
    await this.refreshJoins();
  }

  /** Re-read only the waiting join requests (cheaper than the whole screen). */
  private async refreshJoins(): Promise<void> {
    try {
      this.joins = await store.repo.joinRequests();
    } catch {
      // Sync may be off, or there may be no child yet; either way there is
      // nothing waiting to be shown.
      this.joins = [];
    }
    this.renderJoins();
  }

  /**
   * Devices asking to be let in.
   *
   * All a request carries is a device id — no name travels with it, and none can
   * be invented — so the row shows the id until they are in and have a profile.
   */
  private renderJoins(): void {
    const slot = this.querySelector<HTMLElement>('[data-slot="joins"]');
    if (!slot) return;

    const waiting = (this.joins ?? []).filter((join) => join.state === "pending");
    if (!waiting.length) {
      slot.replaceChildren();
      slot.hidden = true;
      return;
    }

    slot.hidden = false;
    slot.replaceChildren(
      ...waiting.map((join) => {
        const row = document.createElement("div");
        row.className = "baby-join";

        const who = document.createElement("span");
        who.className = "baby-join-who";
        who.textContent = `${join.device.slice(0, 8)}… wants to join`;
        row.append(who);

        const actions = document.createElement("div");
        actions.className = "baby-join-actions";
        for (const [label, accept] of [
          ["Accept", true],
          ["Decline", false],
        ] as const) {
          const button = document.createElement("button");
          button.type = "button";
          button.className = accept ? "chip" : "chip delete-button";
          button.dataset.join = join.device;
          button.dataset.accept = String(accept);
          button.textContent = label;
          actions.append(button);
        }
        row.append(actions);
        return row;
      }),
    );
  }

  private async resolveJoin(device: string, accept: boolean): Promise<void> {
    try {
      await store.repo.resolveJoin(device, accept);
      this.caregivers = null;
      this.joins = null;
      await this.loadCaregivers();
      ui.toast("success", accept ? "They can join now." : "Declined.");
    } catch (error) {
      ui.toast("error", error instanceof Error ? error.message : String(error));
    }
  }

  private renderCaregivers(): void {
    const slot = this.querySelector<HTMLElement>('[data-slot="caregivers"]');
    if (!slot) return;
    if (!this.caregivers || !this.caregivers.length) {
      const empty = document.createElement("p");
      empty.className = "note";
      empty.textContent = "No caregivers yet.";
      slot.replaceChildren(empty);
      return;
    }
    slot.replaceChildren(
      ...this.caregivers.map((caregiver) => {
        const row = new LlCaregiverRow();
        row.setAttribute("name", caregiver.name);
        row.setAttribute("role", caregiver.role);
        return row;
      }),
    );
  }

  private async export(event: MouseEvent): Promise<void> {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-action='export']");
    if (!button) return;
    let csv = "";
    try {
      csv = await store.repo.csv();
    } catch (error) {
      window.alert(`Export failed: ${error instanceof Error ? error.message : error}`);
      return;
    }
    if (await copyText(csv)) {
      button.textContent = "Copied ✓";
      return;
    }
    const url = URL.createObjectURL(new Blob([csv], { type: "text/csv" }));
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "little-log.csv";
    anchor.click();
    button.textContent = "Downloaded ✓";
  }

  private async deleteAll(event: MouseEvent): Promise<void> {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-action='delete-all']");
    if (!button) return;
    if (!this.deleteArmed) {
      this.deleteArmed = true;
      button.textContent = "Tap again to confirm";
      return;
    }
    this.deleteArmed = false;
    try {
      await store.deleteAll();
      button.textContent = "Delete everything";
    } catch {
      button.textContent = "Failed — try again";
    }
  }
}

defineComponent("ll-baby", LlBaby);
