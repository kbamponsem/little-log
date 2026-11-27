---
id: decision-2
title: Resolve each entry with a last-writer-wins register ordered by (HLC, author)
date: '2026-11-27T08:44:43Z'
status: accepted
---
## Context

<!-- SECTION:CONTEXT:BEGIN -->
Two devices can edit or delete the same entry while offline, so every replica must resolve the conflict identically without a server and without a global clock (PLAN.md §3.2).
<!-- SECTION:CONTEXT:END -->

## Decision

<!-- SECTION:DECISION:BEGIN -->
Keep the log as a grow-only set, and resolve each entry id with a single last-writer-wins register over the whole entry state, payload and tombstone together, ordered by (hlc, author) with the author key bytes breaking HLC ties. Arrival order never decides a conflict.
<!-- SECTION:DECISION:END -->

## Consequences

<!-- SECTION:CONSEQUENCES:BEGIN -->
A delete cannot be resurrected by a late edit, states cannot oscillate, and every device converges identically. Edits and deletes are whole-entry operations, not per-field, and a member can win any conflict by claiming a far-future timestamp.
<!-- SECTION:CONSEQUENCES:END -->

## Alternatives

<!-- SECTION:ALTERNATIVES:BEGIN -->
Per-field last-writer-wins. Vector clocks that surface conflicts to the user. A full CRDT per field.
<!-- SECTION:ALTERNATIVES:END -->

