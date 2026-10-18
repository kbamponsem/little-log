import { defineComponent, LLComponent } from "../../core/component";
import { esc } from "../../format";
import { ic } from "../../icons";
import "./ll-card.css";

/**
 * A rounded section card. Keeps the children you nest inside it:
 *
 *   <ll-card heading="Caregivers" heading-icon="baby">
 *     <p class="note">…</p>
 *   </ll-card>
 */
export class LlCard extends LLComponent {
  protected preservesChildren = true;

  protected template(): string {
    const heading = this.getAttribute("heading");
    const icon = this.getAttribute("heading-icon");
    const head = heading
      ? `<h2 class="ch">${icon ? ic(icon, 22) : ""}${esc(heading)}</h2>`
      : "";
    return `${head}<div class="card-body" data-content></div>`;
  }
}

defineComponent("ll-card", LlCard);
