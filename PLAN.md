# Little Log — plan

A local-first baby tracker for feeds, sleep and diapers. Every device holds a
full replica and works offline; caregivers' devices replicate **directly with
each other**. There is no central server and no account system.

One Rust codebase serves an Android app with a home-screen widget, iOS and
desktop later, and an optional always-on replica you can run in Docker.

## 1. Architecture

**Local-first, serverless, gossip replication.**

- Each device keeps a complete copy of the event log and the SQLite projection
  built from it, so everything works with no network at all.
- Devices replicate peer to peer: QUIC with hole punching and relay fallback.
- No central authority, no user accounts, no REST API. Membership and roles are
  cryptographic, not server-side.
- An **always-on peer** (headless, built from the same crates) is optional. It
  holds an encrypted replica and has no special authority — it is just a device
  that is always reachable.

**Removed from the previous design:** `crates/server` (axum, REST sync, Argon2
accounts, invite links, sessions) and the browser as a deployment target. The
`web/` directory survives as the Tauri app's frontend — see §2.

## 2. Workspace

| Path | What it is |
|---|---|
| `crates/core` | Event types, enums, validation, the hybrid logical clock, the append-only event log, the SQLite projection, permissions, CSV export, photo resizing, the widget queue |
| `crates/sync` | Peer networking: QUIC transport, gossip fan-out, anti-entropy, discovery, relay configuration. **Phase 2 spike decides the library.** |
| `app/` | Tauri 2 (Android first, then iOS and desktop) plus the Glance widget |
| `web/` | The Tauri frontend: TypeScript custom elements, built by Vite into `web/dist` and loaded as `frontendDist`. **Not** a standalone browser app any more |
| `peer/` | Headless replica binary, reusing `crates/core` + `crates/sync` (phase 5) |
| `deploy/` | Terraform + Docker for the peer and a self-hosted iroh relay (phase 5) |

`crates/core` must never depend on Tauri, the network, or any UI. Everything
above it is a front end.

## 3. Data model

**Events are append-only and immutable.** The log is a grow-only set; nothing is
ever edited or removed from it. Ids are UUIDv7, generated on the device.

Event kinds: `create`, `edit`, `delete`, `sleep_start`, `sleep_stop`.

Every event carries:

- `id` — UUIDv7
- `topic` — the baby it belongs to
- `author` — the device's ed25519 public key id
- `hlc` — the hybrid logical clock timestamp (§3.1)
- `kind`
- `payload` — encrypted (§3.3)
- `signature` — over the headers **and** the ciphertext

The headers stay readable so peers can sync without the group key. Because the
headers are inside the signature, a peer cannot re-attribute or re-time a
validly-signed event.

### 3.1 Clock

A hybrid logical clock: `(wall_ms, counter, node_id)`. It is monotonic even when
the wall clock goes backwards, and merging a remote timestamp advances the local
one. `node_id` breaks ties between devices that tick in the same millisecond.

### 3.2 Merge

Merge is **set union of events** — inserting an event twice is a no-op, which
makes retries safe.

Resolution per entry id is a **single last-writer-wins register over the whole
entry state** (payload *and* tombstone flag), ordered by `(hlc, author)`:

- Edits and deletes compete in the same register, so a delete cannot be
  resurrected by a late edit and the two states cannot oscillate.
- A tie on HLC is broken by comparing the author key bytes, so every device
  independently reaches the same winner. HLC equality must never be resolved by
  arrival order.

A `delete` is a tombstone event. `delete` of everything is just a tombstone per
entry, exactly as today.

### 3.3 Encryption

Payloads are sealed with the per-baby group key using XChaCha20-Poly1305; the
ciphertext authenticates the headers as associated data. The group key is
rotated when a member is removed (§5).

### 3.4 Sleep sessions

A sleep is a **session** with its own id. `sleep_start` and `sleep_stop` are
events keyed by that id, so two devices acting concurrently converge:

- `sleep_stop` is idempotent — a second stop for the same session is a no-op,
  not a second wake-up.
- `running_sleep()` is derived: the newest session with no stop.

### 3.5 Photos

Resized JPEGs, content-addressed, stored as blobs and fetched lazily, cached on
disk by hash. The event payload holds the hash, not the bytes.

## 4. Sync

- **Live fan-out:** one gossip topic per baby.
- **Anti-entropy:** on every connection and at app start, reconcile ranges (or
  exchange version vectors) so a device that was offline catches up. Gossip is
  lossy and unordered — never rely on it alone for correctness.
- **Discovery:** mDNS on the LAN, plus the relay and peer addresses carried in
  the invite. The relay URL is configurable; a self-hosted relay is supported.
- **Android:** sync on app open and on a WorkManager schedule. The widget writes
  events locally and they replicate when a connection exists.

## 5. Members, invites, roles

**Identity — there is no login and no account.** A device's ed25519 keypair *is*
its identity: generated the first time it opens, kept on the device (SQLite for
now, behind a `KeyStore` trait so an OS keystore — and the peer's file/env
backend — can slot in later), and named by the hex of its public half.
`Event::author` carries it, so a peer verifies an event without asking anyone,
and `EventLog::insert` refuses anything whose signature does not check out.

**Losing the key loses the identity**, deliberately and with no recovery: the
log survives on other devices, and already-replicated events keep verifying
under the old public key, but that device can never author as it again and any
role certificate bound to it must be re-issued. There is no reset because there
is no server to ask. Key export/import is out of scope.

- The **first device** creates the baby, the group key and the owner key.
- An **invite** is a QR code or link carrying the topic, the group key, the
  owner's address and a role. The owner signs a **member certificate** binding
  the new device key to that role.
- **Roles** (co-parent: edit all; caregiver: view and add) are enforced on every
  device when an event is validated, using the existing `permissions::can`
  matrix with `is_author` derived from the event's author.
- **Removal** rotates the group key and re-shares it with the remaining devices,
  and publishes an owner-signed revocation event.

> **Removal is forward secrecy only.** The removed device keeps every event it
> has already decrypted, and its device key remains valid in the log until peers
> honour the revocation. A peer that is offline during the removal may keep
> accepting that member's events for a while. This is inherent to the design and
> must be documented, not discovered.

## 6. Known limitations and non-goals

- **Clocks are trusted.** HLC makes ordering deterministic, but any member can
  win every conflict by claiming a far-future timestamp. Acceptable for a family
  app; it is a non-goal to defend against a malicious member.
- **No key recovery.** Losing a device key loses that identity — no reset, no
  backup, no export (see §5). The data survives; the identity does not.
- **No consensus and no global time.** Peers converge, they do not agree on a
  linear history.
- **Revocation is eventually consistent**, per §5.
- Photos are not deleted when their entry is tombstoned; blob garbage
  collection is not in scope yet.

## 7. Phases

1. **core** — event log, HLC, projection, permissions, with tests for merge,
   conflicts, HLC and permissions. *No networking.* ← current
2. **SPIKE, before any UI work** — two devices (phone + laptop) exchanging events
   by gossip and anti-entropy across different networks, and a device catching up
   after being offline. Pick **iroh** (QUIC + `iroh-gossip` + `iroh-blobs`) or
   **p2panda-net** and justify it in writing.
3. **Tauri app** — storage adapter moves from HTTP/IPC to core in-process;
   Android widget taps log locally and refresh.
4. **Invites, roles, key rotation, photos.**
5. **Peer container** and self-hosted relay, then iOS and desktop.

## 8. Verification

```
cargo test --workspace                     # core: unit + integration
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cd web && npm run build                    # the app frontend still builds
cd app && npm run android:build -- --apk    # phase 3 onward
```

Phase 1 is done when `cargo test --workspace` covers, at minimum: HLC
monotonicity and merge, set-union idempotency, LWW convergence from both
directions, delete-vs-edit precedence, concurrent sleep stop, and role
enforcement rejecting an unauthorised event.

## 9. Carried over from the previous design

- **Kept:** the domain enums, validation, the permissions matrix, CSV export,
  photo resizing, the widget queue, and the whole `web/` component library.
- **Replaced:** `crates/core::sync` (cursor + LWW on `updated_at`) becomes the
  event log + HLC. `crates/core::repo` becomes a *projection*: rebuildable from
  the log, not the source of truth. Its `sessions`, `invites` and
  `password_hash` columns exist only for the deleted server and go away.
- **Still valid:** the widget spike conclusion (own Glance widget + WorkManager
  drain; `tauri-plugin-widgets` rejected) — phase 3.
