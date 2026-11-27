---
id: LL-4
title: Implement group key rotation and revocation when a member is removed
status: To Do
assignee: []
created_date: '2026-11-27T08:44:20Z'
labels: []
dependencies: []
references:
  - PLAN.md
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
PLAN.md §5 says removal must rotate the per-baby group key, re-share it with the remaining devices, and publish an owner-signed revocation event. No rotation or revocation code exists in crates/core or crates/sync yet, so a removed device keeps a working key and peers have nothing to honour. §5 also requires the forward-secrecy-only limitation to be documented rather than discovered.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Removing a member rotates the group key and re-shares it with the remaining devices
- [ ] #2 An owner-signed revocation event is published and honoured by peers that receive it
- [ ] #3 A removed member cannot decrypt events written after the rotation
- [ ] #4 The forward-secrecy-only limitation and the offline-peer window are documented
- [ ] #5 Tests cover rotation, revocation acceptance, and rejection after revocation
<!-- AC:END -->
