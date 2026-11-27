---
id: LL-8
title: Replace the amount stepper with a scroll picker
status: Done
assignee:
  - '@kbamponsem'
created_date: '2026-11-27T09:16:17Z'
updated_date: '2026-11-27T16:09:54Z'
labels: []
dependencies: []
references:
  - web/src/components/ui/ll-stepper.ts
  - web/src/components/sheets/ll-feed-sheet.ts
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Minutes and millilitres are set with ll-stepper, a minus button, an output and a plus button that moves one minute or 10 ml per tap. Reaching a typical value takes many taps, one-handed use is awkward, and there is no way to move quickly. The user asked for a scroll control instead. The stepper is only used by the feed sheet, so the change is contained, but the saved value and the ll-step event the sheet listens for must keep working.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A scrollable, snapping picker replaces the minus and plus buttons in the feed sheet
- [x] #2 Values move in the same steps as today (1 minute, 10 ml), stay within sensible bounds, and the current value is selected when the sheet opens
- [x] #3 The feed sheet still receives the chosen value and saving is unchanged
- [x] #4 The picker is usable by touch and by keyboard, exposes its value and bounds to assistive tech, and does not trap focus
- [x] #5 ll-stepper is either removed or left unused, with no dead import left behind
- [ ] #6 Manual check in the Tauri app: scroll to a value, save, and confirm the stored amount matches
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace ll-stepper with a scroll-snap picker custom element that emits the same ll-step event.\n2. Preserve 1-minute and 10-ml steps, current selection, value bounds, keyboard control, and assistive-tech semantics.\n3. Wire the feed sheet to the new picker, then remove the unused stepper component and its export.\n4. Build the web app and manually verify scrolling, saving, and the stored amount in the Tauri Android app.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented ll-amount-picker: horizontal scroll-snap strip, 1-minute and 10-ml steps, 0..180 min / 0..2000 ml bounds matching core, initial value centred, touch swipe and arrow/Page/Home/End keyboard support, and spinbutton ARIA value/bounds/text. Feed sheet still listens for ll-step and saves the same amount fields. Removed ll-stepper and its export. Verified with tsc/Vite build, web build via the Android package, emulator screenshot, touch swipe, keyboard events, and ARIA inspection. Manual save check (#6) was not performed per the user's request not to touch the live sheets.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the feed sheet's minus/plus stepper with a horizontal snapping amount picker. It preserves the existing 1-minute and 10-ml steps, the 0..180 min and 0..2000 ml core bounds, the current value, and the ll-step contract; it supports touch, keyboard, and assistive technology. The unused ll-stepper component and export were removed. Verified by TypeScript/Vite build, Android debug build, and emulator checks of scrolling, keyboard changes, unit switching, bounds, and ARIA. The manual save confirmation (AC #6) was waived by the user to avoid touching the live feed sheet.
<!-- SECTION:FINAL_SUMMARY:END -->
