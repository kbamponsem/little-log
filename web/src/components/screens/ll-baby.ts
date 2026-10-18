import { defineComponent, LLComponent } from "../../core/component";
import { ui } from "../../core/ui-store";
import type { InviteRole } from "../../core/ui-store";
import type { Baby, Caregiver } from "../../data/model";
import { INVITE_ROLES, THEMES } from "../../data/model";
import { store } from "../../data/store";
import { esc } from "../../format";
import { LlCaregiverRow } from "../ui/ll-caregiver-row";
import type { LlChipGroup } from "../ui/ll-chip-group";
import "./ll-baby.css";

/** Baby profile, caregivers, appearance and data controls. */
export class LlBaby extends LLComponent {
  private caregivers: Caregiver[] | null = null;
  private saveTimer = 0;
  private deleteArmed = false;

  protected template(): string {
    const baby = store.baby;
    return `
      <h1>Baby</h1>
      <ll-card class="card">
        <ll-field name="name" label="Name" flush value="${esc(baby.name)}"></ll-field>
        <ll-field name="born" label="Birthday" type="date" value="${esc(baby.born)}"></ll-field>
      </ll-card>
      <ll-card class="card" heading="Caregivers">
        <div data-slot="caregivers"></div>
        <div class="grp">Invite someone</div>
        <ll-chip-group data-role="invite-role"></ll-chip-group>
        <button class="chip invite-button" type="button" data-action="invite">Copy invite link</button>
        <p class="note">Co-parents can edit everything; caregivers can view and add entries.</p>
      </ll-card>
      <ll-card class="card" heading="Appearance">
        <ll-chip-group data-role="theme"></ll-chip-group>
      </ll-card>
      <ll-card class="card" heading="Your data">
        <div class="chips">
          <button class="chip" type="button" data-action="export">Copy as CSV</button>
          <button class="chip delete-button" type="button" data-action="delete-all">Delete everything</button>
        </div>
        <p class="note">Everything is stored on this device; it syncs to your server when you are signed in.</p>
      </ll-card>
    `;
  }

  protected mount(): void {
    this.querySelector<LlChipGroup>('[data-role="invite-role"]')?.configure({
      options: INVITE_ROLES,
      value: ui.inviteRole,
      toggle: false,
    });
    this.querySelector<LlChipGroup>('[data-role="theme"]')?.configure({
      options: THEMES,
      value: store.baby.theme,
      toggle: false,
    });
    this.renderCaregivers();
  }

  protected subscribe(): void {
    this.watch(store);
    void this.loadCaregivers();

    this.addEventListener("input", (event) => {
      const control = event.target as HTMLInputElement;
      if (control.name === "name") this.saveSoon({ name: control.value });
      if (control.name === "born") this.saveSoon({ born: control.value });
    });

    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "invite") void this.invite(event);
      if (action === "export") void this.export(event);
      if (action === "delete-all") void this.deleteAll(event);
    });

    this.addEventListener("ll-change", (event) => {
      const role = (event.target as HTMLElement).dataset.role;
      const value = (event as CustomEvent<{ value: string | null }>).detail.value;
      if (role === "invite-role" && value) ui.setInviteRole(value as InviteRole);
      if (role === "theme" && value) void store.saveBaby({ theme: value as Baby["theme"] });
    });
  }

  private saveSoon(patch: Partial<Baby>): void {
    window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => {
      void store.saveBaby(patch).catch(() => undefined);
    }, 400);
  }

  private async loadCaregivers(): Promise<void> {
    if (this.caregivers) return;
    try {
      this.caregivers = await store.repo.caregivers();
    } catch {
      this.caregivers = [];
    }
    this.renderCaregivers();
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

  private async invite(event: MouseEvent): Promise<void> {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-action='invite']");
    if (!button) return;
    try {
      const result = await store.repo.invite(ui.inviteRole as "co_parent" | "caregiver");
      const link = `${location.origin}${location.pathname}?invite=${encodeURIComponent(result.token)}`;
      try {
        await navigator.clipboard.writeText(link);
        button.textContent = "Copied ✓";
      } catch {
        window.prompt("Copy your invite link:", link);
        button.textContent = "Copied ✓";
      }
    } catch {
      button.textContent = "Failed — try again";
    }
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
    try {
      await navigator.clipboard.writeText(csv);
      button.textContent = "Copied ✓";
      return;
    } catch {
      /* fall through to download */
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
