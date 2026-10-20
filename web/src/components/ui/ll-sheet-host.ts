import { defineComponent, LLComponent } from "../../core/component";
import type { SheetElement } from "../../core/component";
import { $, focusInside } from "../../core/dom";
import { ui, type SheetRequest } from "../../core/ui-store";
import "./ll-sheet-host.css";

const SHEET_TAGS: Record<SheetRequest["name"], string> = {
  feed: "ll-feed-sheet",
  diaper: "ll-diaper-sheet",
  sleep: "ll-sleep-sheet",
  detail: "ll-detail-sheet",
  babies: "ll-babies-sheet",
  "add-baby": "ll-add-baby-sheet",
  invite: "ll-invite-sheet",
  join: "ll-join-sheet",
  theme: "ll-theme-sheet",
};

/** Sheets that take configuration from the request before they open. */
interface ConfigurableSheet extends SheetElement {
  entryId?: string | null;
}

/**
 * The bottom-sheet host. It owns the scrim, the sliding `.sheet` container,
 * focus handling and Escape/scrim dismissal.
 *
 * It follows `ui.sheet`: when a request appears it mounts the matching sheet
 * component, when the request clears it unmounts it. Sheets close themselves
 * with `ui.closeSheet()` (see core/sheet.ts).
 */
export class LlSheetHost extends LLComponent {
  private mounted: SheetElement | null = null;
  private rendered: SheetRequest | null = null;
  private opener: HTMLElement | null = null;

  protected template(): string {
    return `<div class="scrim" data-scrim></div><section class="sheet" data-sheet></section>`;
  }

  protected mount(): void {
    $("[data-scrim]", this)?.addEventListener("click", () => {
      if (this.mounted?.dismissible !== false) ui.closeSheet();
    });
  }

  protected subscribe(): void {
    this.watch(ui, () => this.sync());
    document.addEventListener("keydown", this.onKeydown);
    this.sync();
  }

  protected disconnect(): void {
    document.removeEventListener("keydown", this.onKeydown);
  }

  private readonly onKeydown = (event: KeyboardEvent): void => {
    if (event.key !== "Escape" || !this.mounted) return;
    if (this.mounted.dismissible !== false) ui.closeSheet();
  };

  /** Mount or unmount whatever `ui.sheet` asks for. */
  private sync(): void {
    const request = ui.sheet;
    if (request === this.rendered) return; // only react to real open/close
    this.rendered = request;
    this.teardown();
    if (request) this.open(request);
  }

  private open(request: SheetRequest): void {
    const container = $<HTMLElement>("[data-sheet]", this);
    if (!container) return;

    this.opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;

    const sheet = document.createElement(SHEET_TAGS[request.name]) as ConfigurableSheet;
    sheet.dataset.sheetName = request.name;
    if (request.entryId !== undefined) sheet.entryId = request.entryId;

    container.append(sheet);
    this.mounted = sheet;
    container.classList.add("open");
    $("[data-scrim]", this)?.classList.add("open");

    const selector = sheet.autofocusSelector ?? "#close";
    if (!focusInside(sheet, selector)) focusInside(sheet, "input,textarea,#close");
  }

  private teardown(): void {
    const sheet = this.mounted;
    const container = $<HTMLElement>("[data-sheet]", this);
    this.mounted = null;
    container?.classList.remove("open");
    $("[data-scrim]", this)?.classList.remove("open");
    if (!sheet) return;
    sheet.remove();
    sheet.onSheetClosed?.();
    if (this.opener?.isConnected) this.opener.focus({ preventScroll: true });
    this.opener = null;
  }
}

defineComponent("ll-sheet-host", LlSheetHost);
