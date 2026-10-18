/** Tiny DOM helpers shared by every Little Log component. */

/** First element matching `selector`. */
export function $<T extends HTMLElement = HTMLElement>(
  selector: string,
  root: ParentNode = document,
): T | null {
  return root.querySelector<T>(selector);
}

/** Every element matching `selector`, as a real array. */
export function $$<T extends HTMLElement = HTMLElement>(
  selector: string,
  root: ParentNode = document,
): T[] {
  return Array.from(root.querySelectorAll<T>(selector));
}

/** Replace the contents of the first `[data-slot="name"]` element. */
export function fill(
  root: ParentNode,
  name: string,
  ...nodes: Array<Node | string>
): HTMLElement | null {
  const slot = root.querySelector<HTMLElement>(`[data-slot="${name}"]`);
  if (!slot) return null;
  slot.replaceChildren(...nodes);
  return slot;
}

/** Append into the first `[data-slot="name"]` element. */
export function appendSlot(
  root: ParentNode,
  name: string,
  ...nodes: Array<Node | string>
): HTMLElement | null {
  const slot = root.querySelector<HTMLElement>(`[data-slot="${name}"]`);
  if (!slot) return null;
  slot.append(...nodes);
  return slot;
}

/** String attribute with a fallback. */
export function strAttr(element: Element, name: string, fallback = ""): string {
  return element.getAttribute(name) ?? fallback;
}

/** Boolean attribute (`<x flag>`). */
export function boolAttr(element: Element, name: string): boolean {
  return element.hasAttribute(name);
}

/** Numeric attribute with a fallback for missing/garbage values. */
export function numAttr(element: Element, name: string, fallback = 0): number {
  const raw = element.getAttribute(name);
  const value = raw === null ? Number.NaN : Number(raw);
  return Number.isFinite(value) ? value : fallback;
}

/** Comma-separated attribute → string list. */
export function listAttr(element: Element, name: string): string[] {
  const raw = element.getAttribute(name);
  return raw
    ? raw
        .split(",")
        .map((part) => part.trim())
        .filter(Boolean)
    : [];
}

/** Build a single element from an HTML string. */
export function fromHtml<T extends HTMLElement = HTMLElement>(markup: string): T {
  const template = document.createElement("template");
  template.innerHTML = markup.trim();
  return template.content.firstElementChild as T;
}

/** Move focus to the first matching control without scrolling the page. */
export function focusInside(root: ParentNode, selector: string): boolean {
  const target = root.querySelector<HTMLElement>(selector);
  if (!target) return false;
  target.focus({ preventScroll: true });
  return true;
}
