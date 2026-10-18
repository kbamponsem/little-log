type Listener<T> = (value: T) => void;

/** Tiny observable: components subscribe, stores notify. */
export class Observable<T> {
  private readonly listeners = new Set<Listener<T>>();

  /** Returns an unsubscribe function. */
  subscribe(listener: Listener<T>): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  protected notify(): void {
    const snapshot = this as unknown as T;
    for (const listener of [...this.listeners]) listener(snapshot);
  }
}
