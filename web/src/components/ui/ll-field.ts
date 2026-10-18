import { defineComponent, LLComponent } from "../../core/component";
import { $, boolAttr, numAttr } from "../../core/dom";
import { esc } from "../../format";
import "./ll-field.css";

/**
 * A labelled input/textarea.
 *
 * `<ll-field name="note" label="Notes" multiline rows="2"></ll-field>`
 *
 * `value` is a property that reads/writes the inner control, so callers never
 * touch `querySelector("input")`.
 */
export class LlField extends LLComponent {
  protected template(): string {
    const label = esc(this.getAttribute("label") ?? "");
    const name = esc(this.getAttribute("name") ?? "");
    const value = esc(this.getAttribute("value") ?? "");
    const multiline = boolAttr(this, "multiline");
    const flush = boolAttr(this, "flush") ? ` style="margin-top:0"` : "";
    const labelHtml = label ? `<div class="grp"${flush}>${label}</div>` : "";

    if (multiline) {
      const rows = numAttr(this, "rows", 2);
      const placeholder = esc(this.getAttribute("placeholder") ?? "");
      return `${labelHtml}<textarea class="inp" name="${name}" rows="${rows}" placeholder="${placeholder}">${value}</textarea>`;
    }

    const type = esc(this.getAttribute("type") ?? "text");
    const placeholder = esc(this.getAttribute("placeholder") ?? "");
    const autocomplete = esc(this.getAttribute("autocomplete") ?? "off");
    return `${labelHtml}<input class="inp" name="${name}" type="${type}" value="${value}" placeholder="${placeholder}" autocomplete="${autocomplete}">`;
  }

  private control(): HTMLInputElement | HTMLTextAreaElement | null {
    return $("input,textarea", this);
  }

  get value(): string {
    return this.control()?.value ?? "";
  }

  set value(next: string) {
    const control = this.control();
    if (control) control.value = next;
  }
}

defineComponent("ll-field", LlField);
