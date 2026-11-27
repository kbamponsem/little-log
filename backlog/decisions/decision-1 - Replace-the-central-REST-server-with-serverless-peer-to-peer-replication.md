---
id: decision-1
title: Replace the central REST server with serverless peer-to-peer replication
date: '2026-11-27T08:44:41Z'
status: accepted
---
## Context

<!-- SECTION:CONTEXT:BEGIN -->
The previous design ran an axum server with Argon2 accounts, invite links, sessions and a browser deployment (PLAN.md §1). It required reachability and coupled every device to one authority, which defeats the offline-first goal for an app used at 3am.
<!-- SECTION:CONTEXT:END -->

## Decision

<!-- SECTION:DECISION:BEGIN -->
Devices hold a full replica of an append-only, signed, encrypted event log and replicate directly over QUIC. There is no central authority, no account system and no REST API. The optional always-on peer is just another device with no special authority.
<!-- SECTION:DECISION:END -->

## Consequences

<!-- SECTION:CONSEQUENCES:BEGIN -->
Everything works offline and no server has to be trusted. Losing a device key loses that identity with no recovery, revocation is only eventual, and no one arbitrates conflicts, which is why merge must be deterministic.
<!-- SECTION:CONSEQUENCES:END -->

## Alternatives

<!-- SECTION:ALTERNATIVES:BEGIN -->
Keep the axum server as the source of truth. Hybrid local cache that syncs through a server.
<!-- SECTION:ALTERNATIVES:END -->

