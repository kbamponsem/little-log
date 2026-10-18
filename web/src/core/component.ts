import type { Observable } from "./observable";

/** Everything a bottom sheet has to expose to the sheet host. */
export interface SheetElement extends HTMLElement {
  /** false → scrim clicks and Escape do not close it (used by the auth sheet). */
  dismissible?: boolean;
  /** CSS selector focused when the sheet opens. */
  autofocusSelector?: string;
  /** Called after the sheet has been removed from the DOM. */
  onSheetClosed?(): void;
}

/**
 * Base class for every Little Log web component.
 *
 * Components render declaratively from `template()` (a string) and wire their
 * events in `mount()`. They keep their light DOM (no shadow root) so the
 * stylesheets and the prototype's layout keep working; components declare
 * `display: contents` in their own CSS file so their inner elements take part
 * in the parent's flex/grid layout.
 *
 * Components can watch stores with `watch()`; on every change `refresh()`
 * re-renders unless the user is mid-edit.
 */
export abstract class LLComponent extends HTMLElement {
  private unsubscribers: Array<() => void> = [];

  /** Set true to keep the children written in HTML (they land in `[data-content]`). */
  protected preservesChildren = false;

  /** The component's markup. */
  protected abstract template(): string;

  /** Wire events here; runs after every render. */
  protected mount(): void {}

  /** Store-change handler; re-render unless focus is in a text field. */
  protected refresh(): void {
    if (!this.isEditing()) this.render();
  }

  /** Subscribe to stores here (`this.watch(store)`). */
  protected subscribe(): void {}

  /** Clean up anything not covered by `watch()`. */
  protected disconnect(): void {}

  connectedCallback(): void {
    this.render();
    this.subscribe();
  }

  disconnectedCallback(): void {
    this.disconnect();
    for (const off of this.unsubscribers) off();
    this.unsubscribers = [];
  }

  /** Re-render, then run `mount()`. */
  render(): void {
    const children = this.preservesChildren ? Array.from(this.childNodes) : [];
    this.innerHTML = this.template();
    if (children.length) {
      const target = this.querySelector<HTMLElement>("[data-content]") ?? this;
      for (const node of children) target.append(node);
    }
    this.mount();
  }

  /** Re-render on every observable change (or run a custom listener). */
  protected watch<S>(observable: Observable<S>, listener?: () => void): void {
    this.unsubscribers.push(
      observable.subscribe(() => {
        if (listener) listener();
        else this.refresh();
      }),
    );
  }

  /** Components opt into attribute reactivity by declaring `observedAttributes`. */
  attributeChangedCallback(
    _name: string,
    _previous: string | null,
    _next: string | null,
  ): void {}

  protected $(selector: string): HTMLElement | null {
    return this.querySelector<HTMLElement>(selector);
  }

  protected $$(selector: string): HTMLElement[] {
    return Array.from(this.querySelectorAll<HTMLElement>(selector));
  }

  protected emit<T>(type: string, detail?: T): void {
    this.dispatchEvent(new CustomEvent<T>(type, { detail, bubbles: true, composed: true }));
  }

  /** Attribute helpers — components stay declarative from HTML. */
  protected attr(name: string, fallback = ""): string {
    return this.getAttribute(name) ?? fallback;
  }

  protected num(name: string, fallback: number): number {
    const value = Number(this.getAttribute(name));
    return Number.isFinite(value) ? value : fallback;
  }

  protected bool(name: string): boolean {
    return this.hasAttribute(name);
  }

  /** Re-render after a property is set programmatically (post-mount). */
  protected rerender(): void {
    if (this.isConnected) this.render();
  }

  protected on<K extends keyof HTMLElementEventMap>(
    target: EventTarget | null,
    type: K,
    handler: (event: HTMLElementEventMap[K]) => void,
  ): void;
  protected on(target: EventTarget | null, type: string, handler: (event: Event) => void): void;
  protected on(
    target: EventTarget | null,
    type: string,
    handler: ((event: Event) => void) | ((event: HTMLElementEventMap[keyof HTMLElementEventMap]) => void),
  ): void {
    target?.addEventListener(type, handler as EventListener);
  }

  /** True while the user is typing inside this component. */
  protected isEditing(): boolean {
    const active = document.activeElement;
    return (
      !!active &&
      this.contains(active) &&
      (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement)
    );
  }
}

/** Register a custom element once. */
export function defineComponent(name: string, ctor: CustomElementConstructor): void {
  if (!customElements.get(name)) customElements.define(name, ctor);
}
