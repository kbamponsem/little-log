import { defineComponent, LLComponent } from "../../core/component";
import { copyText } from "../../core/clipboard";
import { ui } from "../../core/ui-store";
import type { InviteRole } from "../../core/ui-store";
import type { Caregiver, JoinRequest, SyncPeer, SyncStatus, Theme } from "../../data/model";
import { INVITE_ROLES, THEMES } from "../../data/model";
import { store } from "../../data/store";
import { ic } from "../../icons";
import { LlCaregiverRow } from "../ui/ll-caregiver-row";
import type { LlChipGroup } from "../ui/ll-chip-group";
import "./ll-settings.css";

/** "just now", "3m ago", … for the sync card. */
function ago(at: number | null): string {
  if (!at) return "never";
  const seconds = Math.max(0, Math.round((Date.now() - at) / 1000));
  if (seconds < 10) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/**
 * Settings: everything about *this device* and the household it shares with,
 * rather than about the child being logged.
 *
 * The device key, the appearance preference and the note about recovery used to
 * live on the Baby screen, where they sat beside the child's name and birthday
 * as though they belonged to it. Sharing moved here for the same reason: an
 * invite code belongs to the whole log, not to one child's profile.
 */
export class LlSettings extends LLComponent {
  private idRevealed = false;
  private caregivers: Caregiver[] | null = null;
  /** Devices asking to be let in; refreshed with the caregivers. */
  private joins: JoinRequest[] | null = null;
  private joinPoll = 0;
  private syncInfo: SyncStatus | null = null;
  private syncPoll = 0;
  private syncing = false;

  protected template(): string {
    return `
      <div class="settings">
        <header class="settings-head">
          <button class="back" type="button" data-action="close" aria-label="Back">
            ${ic("chevron-left", 24)}
          </button>
          <h1>Settings</h1>
        </header>

        <ll-card class="card" heading="Sharing">
          <div data-slot="joins"></div>
          <div data-slot="caregivers"></div>
          <div class="grp">Invite someone</div>
          <ll-chip-group data-role="invite-role"></ll-chip-group>
          <button class="chip invite-button" type="button" data-action="invite">Show invite code</button>
          <p class="note">Co-parents can edit everything; caregivers can view and add entries. The code carries this child's key, so show it only to someone you trust.</p>
          <div class="grp">Or join someone else's</div>
          <button class="chip join-button" type="button" data-action="join">Join with a code</button>
        </ll-card>

        <ll-card class="card" heading="Sync">
          <p class="sync-line" data-slot="sync-status">Not synced yet.</p>
          <div class="chips">
            <button class="chip" type="button" data-action="sync">Sync now</button>
          </div>
          <div data-slot="peers"></div>
          <p class="note">Sync runs when the app opens and after you log something. A peer is another phone sharing this log.</p>
        </ll-card>

        <ll-card class="card" heading="This device">
          <div class="kv">
            <span class="label">Device ID</span>
            <code class="device-id" data-role="device-id"></code>
          </div>
          <div class="chips">
            <button class="chip" type="button" data-action="reveal-id">Show</button>
            <button class="chip" type="button" data-action="copy-id">Copy</button>
          </div>
          <div class="grp">Appearance</div>
          <ll-chip-group data-role="theme"></ll-chip-group>
          <p class="note">
            This device's key is made here on first open and never leaves it,
            which is why there is no password and no sign-in. It cannot be
            recovered, so keep a second device in sync to keep the log safe.
          </p>
        </ll-card>
      </div>
    `;
  }

  protected mount(): void {
    this.querySelector<LlChipGroup>('[data-role="theme"]')?.configure({
      options: THEMES,
      value: store.theme,
      toggle: false,
    });
    this.querySelector<LlChipGroup>('[data-role="invite-role"]')?.configure({
      options: INVITE_ROLES,
      value: ui.inviteRole,
      toggle: false,
    });
    this.renderDevice();
    this.renderCaregivers();
    this.renderJoins();
    this.renderSync();
  }

  protected subscribe(): void {
    this.watch(store);
    void this.loadCaregivers();
    void this.refreshSync();

    // Join requests are written to the database by the sync listener, not
    // through the store, so nothing here would notice one arriving. While
    // Settings is the view on show, re-read them so a waiting device can
    // actually be let in.
    this.joinPoll = window.setInterval(() => {
      if (!this.hidden && ui.overlay === "settings") void this.refreshJoins();
    }, 3000);

    // Sync progress lives in the Rust service; poll it while this view is up.
    this.syncPoll = window.setInterval(() => {
      if (!this.hidden && ui.overlay === "settings") void this.refreshSync();
    }, 2000);

    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") ui.closeOverlay();
      if (action === "invite") ui.openSheet({ name: "invite" });
      if (action === "join") ui.openSheet({ name: "join" });
      if (action === "sync") void this.forceSync();
      if (action === "reveal-id") {
        this.idRevealed = !this.idRevealed;
        this.renderDevice();
      }
      if (action === "copy-id") void this.copyId(event);
      const join = (event.target as HTMLElement).closest<HTMLElement>("[data-join]");
      if (join?.dataset.join) void this.resolveJoin(join.dataset.join, join.dataset.accept === "true");
    });

    this.addEventListener("ll-change", (event) => {
      const role = (event.target as HTMLElement).dataset.role;
      const value = (event as CustomEvent<{ value: string | null }>).detail.value;
      if (role === "theme" && value) void store.saveTheme(value as Theme);
      if (role === "invite-role" && value) ui.setInviteRole(value as InviteRole);
    });
  }

  protected override disconnect(): void {
    window.clearInterval(this.joinPoll);
    window.clearInterval(this.syncPoll);
    super.disconnect();
  }

  /**
   * Show the device id — the public half of the key. Short by default; the full
   * value is there for comparing two devices, which is the only way to tell one
   * apart from another when there is no account to look at.
   */
  private renderDevice(): void {
    const id = store.deviceId ?? "";
    const slot = this.querySelector<HTMLElement>('[data-role="device-id"]');
    if (slot) {
      slot.textContent = !id
        ? "unavailable"
        : this.idRevealed
          ? id
          : `${id.slice(0, 12)}…${id.slice(-4)}`;
      slot.title = id;
    }
    const button = this.querySelector<HTMLButtonElement>('[data-action="reveal-id"]');
    if (button) button.textContent = this.idRevealed ? "Hide" : "Show";
  }

  private async copyId(event: MouseEvent): Promise<void> {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>(
      "[data-action='copy-id']",
    );
    if (!button || !store.deviceId) return;
    button.textContent = (await copyText(store.deviceId)) ? "Copied ✓" : "Copy failed";
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

  private async refreshSync(): Promise<void> {
    try {
      this.syncInfo = await store.repo.syncStatus();
    } catch {
      this.syncInfo = null;
    }
    this.renderSync();
  }

  private async forceSync(): Promise<void> {
    if (this.syncing) return;
    this.syncing = true;
    this.renderSync();
    try {
      this.syncInfo = await store.repo.forceSync();
      ui.toast("success", "Sync started.");
    } catch (error) {
      ui.toast("error", error instanceof Error ? error.message : String(error));
    } finally {
      this.syncing = false;
      this.renderSync();
    }
  }

  private renderSync(): void {
    const info = this.syncInfo;

    const line = this.querySelector<HTMLElement>('[data-slot="sync-status"]');
    if (line) {
      if (!info) {
        line.textContent = "Sync status unavailable.";
      } else if (info.running) {
        const total = info.peers.length;
        const done = info.peers.filter((peer) => peer.state !== "syncing").length;
        line.textContent = total ? `Syncing… ${done}/${total} peers` : "Syncing…";
      } else if (info.error) {
        line.textContent = info.error;
      } else if (info.last_synced_at) {
        const moved =
          info.received || info.sent ? ` · ${info.received} in, ${info.sent} out` : "";
        line.textContent = `Last synced ${ago(info.last_synced_at)}${moved}`;
      } else {
        line.textContent = "Not synced yet.";
      }
    }

    const button = this.querySelector<HTMLButtonElement>('[data-action="sync"]');
    if (button) {
      const busy = this.syncing || info?.running === true;
      button.disabled = busy;
      button.textContent = busy ? "Syncing…" : "Sync now";
    }

    const slot = this.querySelector<HTMLElement>('[data-slot="peers"]');
    if (!slot) return;

    const peers = info?.peers ?? [];
    if (!peers.length) {
      const empty = document.createElement("p");
      empty.className = "note";
      empty.textContent = "No peers yet. Invite someone, or join their log.";
      slot.replaceChildren(empty);
      return;
    }
    slot.replaceChildren(...peers.map((peer) => this.peerRow(peer)));
  }

  private peerRow(peer: SyncPeer): HTMLElement {
    const row = document.createElement("div");
    row.className = `sync-peer is-${peer.state}`;

    const who = document.createElement("div");
    who.className = "sync-peer-who";
    who.textContent = peer.peer_id
      ? `${peer.peer_id.slice(0, 8)}…${peer.peer_id.slice(-4)}`
      : "Unknown device";
    who.title = peer.addr;

    const meta = document.createElement("div");
    meta.className = "sync-peer-meta";
    meta.textContent = this.peerDetail(peer);

    row.append(who, meta);
    if (peer.error) {
      const error = document.createElement("div");
      error.className = "sync-peer-error";
      error.textContent = peer.error;
      row.append(error);
    }
    return row;
  }

  private peerDetail(peer: SyncPeer): string {
    switch (peer.state) {
      case "syncing":
        return "Syncing…";
      case "error":
        return "Could not reach this device.";
      case "ok":
        return `Last synced ${ago(peer.last_synced_at)} · ${peer.received} in, ${peer.sent} out`;
      default:
        return "Not synced yet";
    }
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
}

defineComponent("ll-settings", LlSettings);
