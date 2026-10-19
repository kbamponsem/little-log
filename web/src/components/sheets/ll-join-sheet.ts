import jsQR from "jsqr";
import { defineComponent } from "../../core/component";
import { LLSheet } from "../../core/sheet";
import { ui } from "../../core/ui-store";
import type { JoinProgress } from "../../data/model";
import "./ll-join-sheet.css";

/** How often to ask whether we have been let in. */
const POLL_MS = 3000;
/** And how long to keep asking before telling the person to try later. */
const WAIT_MS = 5 * 60 * 1000;

/**
 * Join another household: scan the other parent's code, or paste their link.
 *
 * Two ways in on purpose. Scanning is the one a parent will use — it is a single
 * glance, and nothing long-lived ends up in a chat history — but a camera can be
 * refused, missing (an emulator, a tablet with no camera facing out) or simply
 * awkward, so the link always works too.
 */
export class LlJoinSheet extends LLSheet {
  autofocusSelector = "input,textarea";

  private stream: MediaStream | null = null;
  private frame = 0;
  private busy = false;

  protected template(): string {
    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>Join a household</h3>
      <div class="scanner" data-slot="scanner">
        <video data-role="video" playsinline muted></video>
        <p class="note" data-slot="scan-note">Point the camera at the other parent's code.</p>
      </div>
      <div class="grp">Or paste their invite link</div>
      <ll-field name="link" label="Invite link" multiline></ll-field>
      <p class="note" data-slot="status" hidden></p>
      <p class="err" hidden></p>
      <button class="go" type="button" data-action="join">Join</button>
      <button class="chip scan-again" type="button" data-action="scan" hidden>
        Scan again
      </button>
    `;
  }

  protected mount(): void {
    void this.startCamera();
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") this.close();
      if (action === "join") void this.join(this.field("link"));
      if (action === "scan") this.rescan();
    });
  }

  /** The host unmounts us on close; the camera must not outlive that. */
  protected override disconnect(): void {
    this.stopCamera();
  }

  private async startCamera(): Promise<void> {
    this.showScanAgain(false);
    const note = this.querySelector<HTMLElement>('[data-slot="scan-note"]');
    const video = this.querySelector<HTMLVideoElement>('[data-role="video"]');
    if (!video || !navigator.mediaDevices?.getUserMedia) return;

    try {
      this.stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: "environment" },
      });
      video.srcObject = this.stream;
      await video.play();
      this.watchFrames(video);
    } catch {
      // No camera, or the webview refused it: the link below still works.
      this.stream = null;
      if (note) {
        note.textContent =
          "No camera available. Paste the invite link below — it does the same thing.";
      }
    }
  }

  /** Look for a code in each frame until one is found. */
  private watchFrames(video: HTMLVideoElement): void {
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("2d", { willReadFrequently: true });

    const tick = (): void => {
      if (!this.stream || this.busy || !context) return;
      if (video.readyState === video.HAVE_ENOUGH_DATA) {
        canvas.width = video.videoWidth;
        canvas.height = video.videoHeight;
        context.drawImage(video, 0, 0, canvas.width, canvas.height);
        const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
        const code = jsQR(pixels.data, pixels.width, pixels.height, {
          inversionAttempts: "dontInvert",
        });
        if (code?.data) {
          // Keep the scanned link in the field so "Join" can retry it without
          // another scan after a decline.
          this.setLink(code.data);
          void this.join(code.data);
          return;
        }
      }
      this.frame = requestAnimationFrame(tick);
    };

    tick();
  }

  private stopCamera(): void {
    cancelAnimationFrame(this.frame);
    this.frame = 0;
    for (const track of this.stream?.getTracks() ?? []) track.stop();
    this.stream = null;
  }

  /**
   * Ask to join, then wait in the sheet for the answer.
   *
   * The link carries no key, so asking gets us nothing but a place in the queue:
   * someone on the other side has to accept before this device can read a thing.
   * That is why the wait is visible rather than a spinner — "waiting to be
   * accepted" is the honest description of what is happening.
   */
  private async join(link: string | null): Promise<void> {
    if (this.busy) return;
    const trimmed = (link ?? "").trim();
    if (!trimmed) {
      this.error("Scan a code, or paste the invite link.");
      return;
    }

    this.busy = true;
    this.error(null);
    this.stopCamera();
    this.status("Asking…");

    try {
      await this.store.repo.requestJoin(trimmed);
      const answer = await this.waitForAnswer(trimmed);

      if (answer.state === "accepted") {
        await this.store.refresh();
        ui.toast("success", answer.message);
        // If the first-run intro is up, the child has now arrived and there is
        // nothing left to ask — finish it before the sheet unmounts.
        this.emit("ll-welcome-done");
        this.close();
        return;
      }

      // Declined, or nobody answered in time.
      this.status(null);
      this.error(answer.message);
      ui.toast("error", answer.message);
    } catch (error) {
      const message = this.message(error);
      this.status(null);
      this.error(message);
      ui.toast("error", message);
    }

    this.busy = false;
    // Do not restart the camera here: the same code is still in front of it, and
    // rescanning would ask again immediately in a loop. The link stays in the
    // field, and "Scan again" starts a deliberate retry.
    this.showScanAgain(true);
  }

  /** Deliberately scan again after a failed attempt. */
  private rescan(): void {
    if (this.busy) return;
    this.error(null);
    this.showScanAgain(false);
    void this.startCamera();
  }

  /** Keep a scanned invite in the link field, so a retry needs no re-scan. */
  private setLink(link: string): void {
    const input = this.querySelector<HTMLInputElement | HTMLTextAreaElement>(
      "ll-field[name='link'] input, ll-field[name='link'] textarea",
    );
    if (input) input.value = link;
  }

  private showScanAgain(show: boolean): void {
    const button = this.querySelector<HTMLButtonElement>("[data-action='scan']");
    if (button) button.hidden = !show;
  }

  /** Poll until the inviter decides, or give up after a while. */
  private async waitForAnswer(link: string): Promise<JoinProgress> {
    const deadline = Date.now() + WAIT_MS;
    for (;;) {
      const progress = await this.store.repo.pollJoin(link);
      if (progress.state !== "pending") return progress;
      if (Date.now() >= deadline) {
        return { state: "pending", message: "Nobody has accepted yet. Try again shortly." };
      }
      this.status(progress.message);
      await new Promise((resolve) => window.setTimeout(resolve, POLL_MS));
    }
  }

  /** The line under the button: what is happening right now. */
  private status(message: string | null): void {
    const slot = this.querySelector<HTMLElement>('[data-slot="status"]');
    const button = this.querySelector<HTMLButtonElement>("[data-action='join']");
    if (slot) {
      slot.hidden = !message;
      slot.textContent = message ?? "";
    }
    if (button) button.textContent = message ?? "Join";
  }
}

defineComponent("ll-join-sheet", LlJoinSheet);
