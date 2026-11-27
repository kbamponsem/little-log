---
id: LL-12
title: Redesign the home-screen widget around four actions
status: Done
assignee:
  - '@kbamponsem'
created_date: '2026-11-27T14:14:47Z'
updated_date: '2026-11-27T14:53:21Z'
labels: []
dependencies: []
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The widget currently spreads six category buttons across two rows (breast left/right, bottle, pee, poop, sleep toggle), which duplicates choices that belong in the app's sheets and gives sleep two roles in one button. The parent wants a simpler four-button structure: Feed, Diaper, Sleep and Wake Up. Feed and Diaper should open their detail sheets, while Sleep and Wake Up should log immediately without opening the app, matching the Today screen's one-tap sleep tile.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The widget shows exactly four buttons in the order Feed, Diaper, Sleep, Wake Up
- [x] #2 Feed opens the feeding sheet and Diaper opens the diaper sheet
- [x] #3 Tapping Sleep starts a sleep in the background and tapping Wake Up ends the running sleep, without opening the app
- [x] #4 Sleep and Wake Up are safe to repeat: a replay cannot create duplicate sleeps or wake a baby that is not asleep
- [x] #5 The widget redraws after a Sleep or Wake Up tap so the enabled button reflects the new state
- [x] #6 Core tests cover the explicit Sleep and Wake Up actions
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add explicit Sleep and WakeUp variants to core's WidgetAction, parsing their ids and applying them idempotently.\n2. Add widget action ids and a diaper action in Kotlin's WidgetActions; point Feed/Diaper at the app's sheets and Sleep/WakeUp at LogActionCallback.\n3. Redraw the widget as one row of four buttons (Feed, Diaper, Sleep, Wake Up), with the inapplicable sleep action disabled and a sunrise icon for Wake Up.\n4. Map the new Diaper action to the diaper sheet and update launcher-shortcut/widget documentation.\n5. Run core tests, cargo fmt/clippy, and the web build.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Spacing pass: container padding 8dp, three 8dp horizontal Spacers between the four equal-weight tiles, no outer tile gaps. Verified on emulator that each gap and the container inset is 21px at density 2.625 (8dp) and all tiles share equal width. Full debug APK build succeeded; no widget_info change needed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the widget's six category buttons with Feed, Diaper, Sleep and Wake Up. Feed and Diaper launch their existing sheets; Sleep and Wake Up use Glance's action callback and explicit core actions so they log in the background. Added idempotent Sleep/WakeUp handling in core, a reliable per-instance widget refresh path, and 8dp outer/gap spacing. Verified on the arm64 emulator: all four buttons render in order, Feed and Diaper open their sheets, Sleep/Wake Up change the stored state without opening the app, and the widget flips the enabled button after each tap. Core pending tests pass (9/9, including explicit Sleep/Wake idempotency), Kotlin compiles, and the debug APK builds.
<!-- SECTION:FINAL_SUMMARY:END -->
