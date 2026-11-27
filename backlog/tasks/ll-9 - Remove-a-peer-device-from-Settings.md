---
id: LL-9
title: Remove a peer device from Settings
status: To Do
assignee: []
created_date: '2026-11-27T09:16:19Z'
labels: []
dependencies: []
references:
  - crates/core/src/repo.rs
  - crates/sync/src/protocol.rs
  - web/src/components/screens/ll-settings.ts
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Settings lists known peers with sync state but offers no way to remove one. The peers table stores one row per (topic, addr), and both an invite and any inbound connection re-learn an address through save_peer, so deleting a row alone would come back on the next round. Households need a real forget action: stop dialing that device, keep all log history, and do not silently accept it again until it is re-invited.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each peer row in Settings has a Remove action that asks for confirmation and names the device
- [ ] #2 Removing a device deletes every stored address that belongs to it, not just the row that was tapped
- [ ] #3 A removed device is not re-added by an inbound sync round or by retrying an old invite
- [ ] #4 Removal deletes no entries and changes no other device data
- [ ] #5 Tests cover deleting peer rows, deleting multiple addresses for one device, and a round that would otherwise re-learn a removed address
<!-- AC:END -->
