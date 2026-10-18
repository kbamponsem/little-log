import { defineComponent } from "../../core/component";
import { LLSheet } from "../../core/sheet";
import {
  AMOUNTS,
  CONSISTENCIES,
  DIAPER_TYPES,
  PEE_SHADES,
  POOP_COLORS,
  newEntry,
} from "../../data/model";
import type { Amount, Consistency, DiaperType, PeeShade, PoopColor } from "../../data/model";
import { capNow, setT, tv } from "../../format";
import type { LlChipGroup } from "../ui/ll-chip-group";

/** Log a diaper change, optionally with a photo. */
export class LlDiaperSheet extends LLSheet {
  private type: DiaperType = "pee";
  private peeShade: PeeShade | null = null;
  private peeAmount: Amount | null = null;
  private poopColor: PoopColor | null = null;
  private consistency: Consistency | null = null;
  private poopAmount: Amount | null = null;
  private photo: Blob | null = null;

  protected template(): string {
    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>Diaper change</h3>
      <ll-chip-group data-role="type"></ll-chip-group>
      <div data-role="pee">
        <div class="grp">Pee colour</div><ll-chip-group data-role="pee_shade"></ll-chip-group>
        <div class="grp">How much?</div><ll-chip-group data-role="pee_amount"></ll-chip-group>
      </div>
      <div data-role="poop" hidden>
        <div class="grp">Poop colour</div><ll-chip-group data-role="poop_color"></ll-chip-group>
        <div class="grp">Consistency</div><ll-chip-group data-role="consistency"></ll-chip-group>
        <div class="grp">How much?</div><ll-chip-group data-role="poop_amount"></ll-chip-group>
      </div>
      <div class="grp">Photo</div>
      <ll-photo-picker></ll-photo-picker>
      <ll-field name="when" label="When" type="time" value="${tv(Date.now())}"></ll-field>
      <ll-field name="note" label="Notes" multiline></ll-field>
      <p class="err" hidden></p>
      <button class="go" type="button" data-action="save">Log change now</button>
    `;
  }

  protected mount(): void {
    this.group("type")?.configure({ options: DIAPER_TYPES, value: this.type, variant: "big", toggle: false });
    this.group("pee_shade")?.configure({ options: PEE_SHADES, value: null });
    this.group("pee_amount")?.configure({ options: AMOUNTS, value: null });
    this.group("poop_color")?.configure({ options: POOP_COLORS, value: null });
    this.group("consistency")?.configure({ options: CONSISTENCIES, value: null });
    this.group("poop_amount")?.configure({ options: AMOUNTS, value: null });
    this.applyType();
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") this.close();
      if (action === "save") void this.save();
    });

    this.addEventListener("ll-change", (event) => {
      const group = event.target as HTMLElement;
      const role = group.dataset.role ?? "";
      const value = (event as CustomEvent<{ value: string | null }>).detail.value;
      this.assign(role, value);
      if (role === "type") this.applyType();
    });

    this.addEventListener("ll-photo-picked", (event) => {
      this.photo = (event as CustomEvent<{ blob: Blob }>).detail.blob;
    });
  }

  private group(role: string): LlChipGroup | null {
    return this.querySelector<LlChipGroup>(`[data-role="${role}"]`);
  }

  private assign(role: string, value: string | null): void {
    switch (role) {
      case "type":
        this.type = (value ?? "pee") as DiaperType;
        break;
      case "pee_shade":
        this.peeShade = value as PeeShade | null;
        break;
      case "pee_amount":
        this.peeAmount = value as Amount | null;
        break;
      case "poop_color":
        this.poopColor = value as PoopColor | null;
        break;
      case "consistency":
        this.consistency = value as Consistency | null;
        break;
      case "poop_amount":
        this.poopAmount = value as Amount | null;
        break;
    }
  }

  private applyType(): void {
    const pee = this.querySelector<HTMLElement>('[data-role="pee"]');
    const poop = this.querySelector<HTMLElement>('[data-role="poop"]');
    if (pee) pee.hidden = this.type === "poop";
    if (poop) poop.hidden = this.type === "pee";
  }

  private async save(): Promise<void> {
    this.error(null);
    const when = this.field("when") || tv(Date.now());
    const note = this.field("note");
    const entry = newEntry({
      kind: "diaper",
      occurred_at: capNow(setT(Date.now(), when)),
      diaper_type: this.type,
      note: note || null,
    });

    if (this.type !== "poop") {
      entry.pee_shade = this.peeShade;
      entry.pee_amount = this.peeAmount;
    }
    if (this.type !== "pee") {
      entry.poop_color = this.poopColor;
      entry.consistency = this.consistency;
      entry.poop_amount = this.poopAmount;
    }

    try {
      if (this.photo) {
        try {
          entry.photo = await this.store.repo.savePhoto(entry.id, this.photo);
        } catch {
          entry.photo = null;
        }
      }
      await this.store.saveEntry(entry);
      this.close();
    } catch (error) {
      this.error(this.message(error));
    }
  }
}

defineComponent("ll-diaper-sheet", LlDiaperSheet);
