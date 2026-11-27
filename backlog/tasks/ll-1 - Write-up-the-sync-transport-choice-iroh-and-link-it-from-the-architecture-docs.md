---
id: LL-1
title: >-
  Write up the sync transport choice (iroh) and link it from the architecture
  docs
status: To Do
assignee: []
created_date: '2026-11-27T08:44:15Z'
labels: []
dependencies: []
references:
  - PLAN.md
  - docs/architecture/README.md
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
PLAN.md §7 (phase 2) requires the spike to pick iroh or p2panda and justify it in writing. crates/sync already builds on iroh 1.3 and iroh-tickets, so the choice exists in code but no written comparison exists anywhere in the repo. Without it, whoever touches sync next cannot tell which constraints (hole punching, relay fallback, blobs, Android) drove the choice, or what was given up.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A document under docs/ compares iroh and p2panda against the constraints in PLAN.md §4 and §7
- [ ] #2 It names the trade-offs accepted and cites the dependency versions in crates/sync/Cargo.toml
- [ ] #3 docs/architecture/README.md links to it from the sync row
- [ ] #4 A Backlog decision record captures the choice
<!-- AC:END -->
