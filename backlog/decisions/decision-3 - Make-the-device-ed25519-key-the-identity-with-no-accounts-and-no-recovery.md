---
id: decision-3
title: Make the device ed25519 key the identity, with no accounts and no recovery
date: '2026-11-27T08:44:44Z'
status: accepted
---
## Context

<!-- SECTION:CONTEXT:BEGIN -->
Without a server there is no account to authenticate against, and a family app has to work with zero setup (PLAN.md §5).
<!-- SECTION:CONTEXT:END -->

## Decision

<!-- SECTION:DECISION:BEGIN -->
A device keypair is the identity. It is generated on first open, kept on the device behind a KeyStore trait, and named by the hex of its public half. EventLog::insert rejects any event whose signature does not verify, and roles are enforced per event.
<!-- SECTION:DECISION:END -->

## Consequences

<!-- SECTION:CONSEQUENCES:BEGIN -->
There is no login flow, and verifying an event needs no third party. Losing the key means that identity can never author again and any role certificate bound to it must be re-issued. Key export and import are out of scope.
<!-- SECTION:CONSEQUENCES:END -->

## Alternatives

<!-- SECTION:ALTERNATIVES:BEGIN -->
Server-issued accounts. A passphrase-derived identity. Identity held only in the OS keystore.
<!-- SECTION:ALTERNATIVES:END -->

