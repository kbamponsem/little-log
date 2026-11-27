---
id: LL-2
title: Add the headless always-on peer binary
status: To Do
assignee: []
created_date: '2026-11-27T08:44:17Z'
labels: []
dependencies: []
references:
  - PLAN.md
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
PLAN.md §2 and §7 (phase 5) describe an optional always-on replica that reuses crates/core and crates/sync, holds an encrypted replica and has no special authority. No peer/ crate exists yet, so today only the Tauri app speaks the sync protocol and there is nothing for a household to leave running.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A peer/ crate builds a headless binary from crates/core and crates/sync with no Tauri or UI dependency
- [ ] #2 It replicates events with at least one app instance
- [ ] #3 Its data directory and relay configuration are documented
<!-- AC:END -->
