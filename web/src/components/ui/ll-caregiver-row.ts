import { defineComponent, LLComponent } from "../../core/component";
import { strAttr } from "../../core/dom";
import { ROLE_LABELS, type Role } from "../../data/model";
import { esc } from "../../format";
import "./ll-caregiver-row.css";

/** `<ll-caregiver-row name="Partner" role="co_parent"></ll-caregiver-row>` */
export class LlCaregiverRow extends LLComponent {
  static observedAttributes = ["name", "role"];

  protected template(): string {
    const role = strAttr(this, "role") as Role;
    return `
      <div class="cg">
        <span>${esc(strAttr(this, "name"))}</span>
        <span>${esc(ROLE_LABELS[role] ?? strAttr(this, "role"))}</span>
      </div>
    `;
  }

  attributeChangedCallback(): void {
    this.rerender();
  }
}

defineComponent("ll-caregiver-row", LlCaregiverRow);
