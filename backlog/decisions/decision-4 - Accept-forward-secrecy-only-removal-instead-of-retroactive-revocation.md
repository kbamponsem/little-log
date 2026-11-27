---
id: decision-4
title: Accept forward-secrecy-only removal instead of retroactive revocation
date: '2026-11-27T08:44:46Z'
status: accepted
---
## Context

<!-- SECTION:CONTEXT:BEGIN -->
An owner must be able to remove a caregiver without a server, but the removed device already holds decrypted history (PLAN.md §5).
<!-- SECTION:CONTEXT:END -->

## Decision

<!-- SECTION:DECISION:BEGIN -->
Removal rotates the group key, re-shares it with the remaining devices and publishes an owner-signed revocation event that peers honour when they see it.
<!-- SECTION:DECISION:END -->

## Consequences

<!-- SECTION:CONSEQUENCES:BEGIN -->
A removed member cannot read events written after the rotation but keeps everything already decrypted, and a peer that is offline during the removal may keep accepting that member for a while. This limitation must be documented, not discovered by users.
<!-- SECTION:CONSEQUENCES:END -->

## Alternatives

<!-- SECTION:ALTERNATIVES:BEGIN -->
Retroactively re-encrypt history. Require a server to mediate removal. Do not support removal.
<!-- SECTION:ALTERNATIVES:END -->

