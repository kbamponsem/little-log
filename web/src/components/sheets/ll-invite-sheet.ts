import QRCode from "qrcode";
import { defineComponent } from "../../core/component";
import { copyText } from "../../core/clipboard";
import { LLSheet } from "../../core/sheet";
import { ui } from "../../core/ui-store";
import { INVITE_ROLES } from "../../data/model";
import { esc } from "../../format";
import "./ll-invite-sheet.css";

/**
 * Show this child's invite: a QR for the other parent to scan, and the link for
 * the times a camera is not to hand.
 *
 * The code is minted in the Rust core (see `little_log_core::invite`): it carries
 * the child, the group key, the role and this device's address. That makes it a
 * secret — anyone holding it can read the child's log — which is why this screen
 * says so rather than calling it a harmless link.
 */
export class LlInviteSheet extends LLSheet {
  /** Nothing to focus: the code is for the other phone's camera. */
  autofocusSelector = "[data-action='copy']";

  private link = "";

  protected template(): string {
    const role = INVITE_ROLES.find((option) => option.id === ui.inviteRole)?.label ?? "Co-parent";
    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>Invite a ${esc(role.toLowerCase())}</h3>
      <div class="invite-code" data-slot="code">
        <canvas data-role="qr" width="260" height="260"></canvas>
        <p class="note" data-slot="status">Making an invite…</p>
      </div>
      <button class="go" type="button" data-action="copy" disabled>Copy the link instead</button>
      <p class="note">Anyone with this code can read and add to this child's log, so show it only to someone you trust.</p>
    `;
  }

  protected mount(): void {
    void this.mint();
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") this.close();
      if (action === "copy") void this.copy();
    });
  }

  private async mint(): Promise<void> {
    const status = this.querySelector<HTMLElement>('[data-slot="status"]');
    try {
      this.link = await this.store.repo.invite(ui.inviteRole);
      const canvas = this.querySelector<HTMLCanvasElement>('[data-role="qr"]');
      if (canvas) {
        // Low error correction on purpose: the payload is long, and a denser
        // code at a higher level gets hard to scan off a phone screen.
        await QRCode.toCanvas(canvas, this.link, {
          errorCorrectionLevel: "L",
          margin: 1,
          width: 260,
          color: { dark: "#0f2418", light: "#ffffff" },
        });
      }
      if (status) status.hidden = true;
      const copy = this.querySelector<HTMLButtonElement>("[data-action='copy']");
      if (copy) copy.disabled = false;
    } catch (error) {
      if (status) {
        status.hidden = false;
        status.textContent = this.message(error);
      }
    }
  }

  private async copy(): Promise<void> {
    const button = this.querySelector<HTMLButtonElement>("[data-action='copy']");
    if (!button || !this.link) return;
    button.textContent = (await copyText(this.link)) ? "Copied ✓" : "Copy failed";
  }
}

defineComponent("ll-invite-sheet", LlInviteSheet);
