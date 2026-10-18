/** Form helpers shared by the sign-in screen and the bottom sheets. */

/** Minimal shape of a labelled field component (`ll-field`). */
export interface FieldLike extends HTMLElement {
  value: string;
}

/** Read a named `ll-field` value (trimmed). */
export function fieldValue(root: ParentNode, name: string): string {
  const element = root.querySelector<FieldLike>(`ll-field[name="${name}"]`);
  return element ? element.value.trim() : "";
}

/** Show, or clear, the inline error line (`.err`) inside `root`. */
export function setError(root: ParentNode, message: string | null): void {
  const node = root.querySelector<HTMLElement>(".err");
  if (!node) {
    if (message) window.alert(message);
    return;
  }
  node.textContent = message ?? "";
  node.hidden = !message;
}

/** Turn any thrown value into a readable string (parses JSON error bodies). */
export function errorMessage(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  try {
    const parsed = JSON.parse(raw) as { error?: string };
    if (parsed && typeof parsed.error === "string") return parsed.error;
  } catch {
    /* not JSON — use the raw message */
  }
  return raw || "Something went wrong";
}
