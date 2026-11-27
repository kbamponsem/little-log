---
id: LL-6
title: Add running feed sessions to core
status: Done
assignee:
  - '@kwabena'
created_date: '2026-11-27T09:16:14Z'
updated_date: '2026-11-27T09:24:31Z'
labels: []
dependencies: []
references:
  - crates/core/src/model.rs
  - crates/core/src/projection.rs
  - PLAN.md
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A feed is currently a point-in-time entry with occurred_at plus kind and amount, so there is no way to record that a feed is still in progress. Sleep already models this as a session with sleep_start and sleep_stop events and a derived running_sleep, including an idempotent stop and convergence when two devices act at once. The user wants start and stop for feeding, so core needs the same session model before any UI can offer a timer. Doing it in core keeps the app, the widget and any future peer on one definition.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Feed entries carry an optional end time, and a feed with no end time is a running feed
- [x] #2 Validation accepts an end time after the start and rejects one at or before it
- [x] #3 Repo exposes running_feed and running_feed_for, scoped per baby like running_sleep
- [x] #4 A stopped feed keeps its kind, amount, unit and note, and its duration is derived from the timestamps
- [x] #5 Two stops written concurrently converge to one newest state with no duplicate entry
- [x] #6 CSV details show the amount when present and the duration otherwise
- [x] #7 Tests cover a running feed after reopening the database, per-baby scoping, and concurrent stops
- [x] #8 cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings and cargo fmt --check pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reuse the existing session pattern instead of adding event kinds: an entry with ended_at None is running, and saving it with ended_at is the stop. SleepStart/SleepStop exist in EventKind but are never authored (record_new_entry always writes Create or Edit), so adding feed kinds would create a second, unused mechanism.\n2. Relax validate_feed to accept an end time after the start, matching validate_sleep.\n3. Add Repo::running_feed and running_feed_for, mirroring the sleep queries.\n4. Add Entry::is_feeding and make feed CSV details fall back to the derived duration when no amount is recorded.\n5. Cover validation, running-feed resolution, per-baby scoping, reopened database and concurrent stops with tests; run fmt, tests and clippy.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Correction to the original task: it assumed new feed_start and feed_stop event kinds were needed. The log never authors SleepStart/SleepStop either; sleep sessions are ordinary entries where ended_at is the running marker, and a stop is an Edit through the same LWW register. Feed sessions should follow that proven path.

Implemented the running-feed marker with the same mechanism sleep uses, not with new event kinds. validate_feed now accepts ended_at after occurred_at; Entry::is_feeding and Repo::running_feed/running_feed_for were added; feed CSV details fall back to the derived duration when no amount was typed. Tests: feed validation, per-baby scoping, a running feed after reopening the database, duration/kind surviving the stop, and concurrent stops from two devices converging in both arrival orders.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added running feed sessions to core using the existing session pattern: a feed with ended_at None is running, and saving it with an end time is the stop. Changed validate_feed to accept an end after the start, added Entry::is_feeding and Repo::running_feed/running_feed_for, and made CSV details fall back to the derived duration when no amount is recorded. No event kinds were added because SleepStart/SleepStop are never authored either; sleep sessions are ordinary entries resolved through the same LWW register, and feeding now follows that proven path. Verification: cargo fmt --check, cargo test --workspace (182 tests) and cargo clippy --workspace --all-targets -- -D warnings all pass. The UI still logs a completed feed only; start/stop in the sheet is LL-7.
<!-- SECTION:FINAL_SUMMARY:END -->
