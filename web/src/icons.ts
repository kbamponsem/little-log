/** Inline SVG icons (from the prototype). */

/**
 * The International Breastfeeding Symbol (Matt Daigle, 2006) — donated to the
 * public domain, so it can ship without an attribution file.
 *
 * The geometry is unmodified: only the colour is inherited rather than fixed,
 * and the badge the original is drawn on is dropped. Its native 1024 grid is
 * scaled to this set's 24, and the baby's head — a circle the original paints
 * in the badge colour to punch through — is a hole here.
 */
const BREAST_FIGURE =
  '<circle cx="512.2" cy="276.4" r="116" fill="currentColor" stroke="none"/>' +
  '<path fill="currentColor" stroke="none" fill-rule="evenodd" d="M610.3,423.8c12.7,0,24.2,1.6,34.4,4.9c18.9,6,33.5,17.2,45,30c19.9,22.2,36.3,54.1,54,86 c16.9,30.4,36.4,63.1,36,104c-0.4,40.7-19.9,70.1-40,92c-27.2,29.7-63.2,65.7-118,65c-27.6-0.4-53.4-15.9-49-48 c3.4-24.8,26.1-35.3,40-52c-68.3,7.1-97.8-27.5-124-69c-10.8-17.1-17.7-42-39-46c-42.5-8-37.4,40.9-27,68c3.4,8.9,14.7,24.8,14,32 c-1.2,12.3-25.5,17.3-39,20c-17.4,3.4-38,2.7-51,8c5.1,10.8,18.7,15.6,30,20c11.4,4.4,24.6,8.1,38,11c30.2,6.5,62.6,2.9,93,6 c28,2.9,60,27.8,44,65c-11.7,27.2-46.9,41.9-85,44c-65.2,3.6-143.6-29.9-177-60c-27.6-24.9-48.5-65.3-45-114 c3.4-47.4,26.4-94,45-131c23.3-46.3,43.9-96,85-121c13.4-8.2,32.7-14.9,55-14.9z M520.2,610.4a80,80 0 1,0 160,0a80,80 0 1,0-160,0"/>';

/**
 * The figure's bounds inside that 1024 grid, measured with `getBBox`, plus a 5%
 * margin — so it sits optically like the stroke icons instead of touching the
 * edges. Cropping to this rather than the original canvas is what stops the
 * symbol looking small in its box.
 */
const BREAST_BOX = "217.5 125.2 588.9 775.1";

/**
 * The symbol is handed to both breast options — mirrored for the right, so the
 * two are distinguishable at a glance. The labels carry the actual meaning.
 */
function breastIcon(mirrored: boolean): string {
  const flip = mirrored ? ' transform="translate(24 0) scale(-1 1)"' : "";
  return `<svg viewBox="${BREAST_BOX}"${flip}>${BREAST_FIGURE}</svg>`;
}

export const IC = {
  milk:
    '<path d="M8 2h8"/><path d="M9 2v2.789a4 4 0 0 1-.672 2.219l-.656.984A4 4 0 0 0 7 10.212V20a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2v-9.789a4 4 0 0 0-.672-2.219l-.656-.984A4 4 0 0 1 15 4.788V2"/><path d="M7 15a6.472 6.472 0 0 1 5 0 6.47 6.47 0 0 0 5 0"/>',
  moon: '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>',
  droplet:
    '<path d="M12 22a7 7 0 0 0 7-7c0-2-1-3.9-3-5.5s-3.5-4-4-6.5c-.5 2.5-2 4.9-4 6.5C6 11.1 5 13 5 15a7 7 0 0 0 7 7z"/>',
  poop:
    '<path d="M5 20h14a2 2 0 0 0 0-4H5a2 2 0 0 0 0 4Z"/><path d="M7 16a2 2 0 0 1 0-4h10a2 2 0 0 1 0 4"/><path d="M9 12c0-3 2-4 3-6 1 1.5 3 3 3 6"/>',
  heart:
    '<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>',
  utensils:
    '<path d="M3 2v7c0 1.1.9 2 2 2h4a2 2 0 0 0 2-2V2"/><path d="M7 2v20"/><path d="M21 15V2a5 5 0 0 0-5 5v6c0 1.1.9 2 2 2h3Zm0 0v7"/>',
  camera:
    '<path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/>',
  baby:
    '<path d="M9 12h.01"/><path d="M15 12h.01"/><path d="M10 16c.5.3 1.2.5 2 .5s1.5-.2 2-.5"/><path d="M19 6.3a9 9 0 0 1 1.8 3.9 2 2 0 0 1 0 3.6 9 9 0 0 1-17.6 0 2 2 0 0 1 0-3.6A9 9 0 0 1 12 3c2 0 3.5 1.1 3.5 2.5s-.9 2.5-2 2.5c-.8 0-1.5-.4-1.5-1"/>',
  house:
    '<path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"/><path d="M3 10a2 2 0 0 1 .709-1.528l7-5.999a2 2 0 0 1 2.582 0l7 5.999A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  "book-open":
    '<path d="M12 7v14"/><path d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"/>',
  "chart-column":
    '<path d="M3 3v16a2 2 0 0 0 2 2h16"/><path d="M18 17V9"/><path d="M13 17V5"/><path d="M8 17v-3"/>',
  "chevron-left": '<path d="m15 18-6-6 6-6"/>',
  "chevron-right": '<path d="m9 18 6-6-6-6"/>',
  breast_left: breastIcon(false),
  breast_right: breastIcon(true),
} as const;

export type IconName = keyof typeof IC;

/** Inline an icon by name at the given pixel size. */
export function ic(name: IconName | string, size: number): string {
  const path = IC[name as IconName];
  if (!path) return "";
  return `<svg class="ic" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${path}</svg>`;
}
