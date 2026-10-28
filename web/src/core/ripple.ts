/**
 * The Material ripple, installed once for the whole app.
 *
 * Every tappable surface here is a `<button>`, so one delegated listener covers
 * the sheet actions, the tabs, the chips, the timeline rows and the tiles. The
 * ripple is a span inside the pressed element: it inherits that element's text
 * colour (so it reads on light and dark fills alike) and is clipped by its
 * radius. It removes itself when its animation ends.
 */

/** Safety net for when `animationend` never arrives (element re-rendered). */
const RIPPLE_MS = 600;

export function installRipple(): void {
  document.addEventListener(
    "pointerdown",
    (event) => {
      if (event.button !== 0) return;
      const button = (event.target as HTMLElement | null)?.closest<HTMLButtonElement>("button");
      if (!button || button.disabled) return;

      const rect = button.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const y = event.clientY - rect.top;
      // Enough to reach the farthest corner from where the finger landed, so
      // the circle finishes exactly as it covers the control.
      const radius = Math.max(
        Math.hypot(x, y),
        Math.hypot(rect.width - x, y),
        Math.hypot(x, rect.height - y),
        Math.hypot(rect.width - x, rect.height - y),
      );
      const size = radius * 2;
      const ripple = document.createElement("span");
      ripple.className = "ripple";
      ripple.style.width = `${size}px`;
      ripple.style.height = `${size}px`;
      ripple.style.left = `${x - radius}px`;
      ripple.style.top = `${y - radius}px`;
      button.append(ripple);

      const remove = () => ripple.remove();
      ripple.addEventListener("animationend", remove, { once: true });
      window.setTimeout(remove, RIPPLE_MS);
    },
    { passive: true },
  );
}
