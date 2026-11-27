---
id: LL-5
title: Update the architecture status table to match the code
status: Done
assignee:
  - '@kwabena'
created_date: '2026-11-27T08:44:22Z'
updated_date: '2026-11-27T08:49:58Z'
labels: []
dependencies: []
references:
  - docs/architecture/README.md
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
docs/architecture/README.md still lists crates/sync (QUIC, gossip, anti-entropy, discovery) as planned for the phase-2 spike, but crates/sync now contains endpoint.rs and protocol.rs on iroh. Readers of the architecture docs get a wrong picture of what already exists, which makes the plan look further behind than it is.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Every status row in docs/architecture/README.md is verifiable from the current tree
- [x] #2 The table records when it was last checked against the code
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Re-check every existing row against the tree, not against PLAN.md.
2. Split the single crates/sync row: iroh endpoint and anti-entropy are implemented; gossip fan-out (§6) and mDNS discovery are not.
3. Correct the store row: schema v11 with the full table set, projection rebuild included, not schema v3.
4. Correct the roles row: matrix is done and unit-tested but is not yet called when an event is validated (the server it was applied in is gone).
5. Add peer/relay rows with their real state, noting deploy/ still provisions the removed server.
6. Record the date the table was last checked, and keep every claim pointing at a path.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Checked each row against the tree rather than the plan. Notable corrections: schema is v11 with entries, babies, caregivers, device_keys, group_keys, peers and join_requests tables plus a rebuildable projection (was "schema v3"); permissions::can is implemented and unit-tested but no production code calls it yet, so the old "applied server-side today" was wrong now that the server is gone; crates/sync implements the iroh endpoint and per-topic version-vector anti-entropy (tests/join.rs), but not the gossip fan-out or mDNS discovery the diagrams show, and not the always-on peer or a self-hosted relay.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rewrote the "What already exists in the code" table in docs/architecture/README.md so each row describes the current tree and names the path it can be checked against. Split the single crates/sync row into endpoint and anti-entropy (both done) versus gossip fan-out and mDNS discovery (not started), corrected the store row to schema v11 with a rebuildable projection, corrected the roles row to matrix-done-but-not-yet-enforced during validation, and added separate rows for the always-on peer and the self-hosted relay, noting that deploy/ still provisions the removed server. Added the date the table was last checked (2026-11-27). Verified with 17 tree checks, one per row plus the date; docs-only change, so no cargo tests were run and no diagrams were regenerated.
<!-- SECTION:FINAL_SUMMARY:END -->
