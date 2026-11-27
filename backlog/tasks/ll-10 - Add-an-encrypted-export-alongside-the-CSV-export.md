---
id: LL-10
title: Add an encrypted export alongside the CSV export
status: In Progress
assignee:
  - '@kwabena'
created_date: '2026-11-27T09:16:21Z'
updated_date: '2026-11-27T14:32:17Z'
labels: []
dependencies: []
references:
  - crates/core/src/csv.rs
  - crates/core/src/crypto.rs
  - web/src/components/screens/ll-baby.ts
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Export today writes plaintext CSV through the save dialog from the baby settings screen. That is fine for a spreadsheet but not for a backup: anyone who gets the file reads the whole family log. The user wants an encrypted export and a matching re-import flow.

Core already has XChaCha20-Poly1305 for payloads but no passphrase KDF. The backup carries the signed event log plus the per-child group keys, not projected rows, so a restore merges events through the same path as sync and rebuilds the projections. Importing rows directly would break the rule that every row is derived from the log.

Import is a merge, never a wipe: re-importing the same file is a no-op, and a backup from another log for the same topic is rejected before anything is written. The device key is not in the backup, so imported events keep their original authors and signatures. Device settings and caregivers are not restored.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The export action offers plain CSV and an encrypted backup
- [ ] #2 The encrypted backup is one file, requires a passphrase, and cannot be read without it
- [ ] #3 A wrong passphrase or a tampered file fails with a clear error and yields no partial data
- [ ] #4 The encrypted backup contains at least every entry the CSV export contains
- [ ] #5 The file format and the passphrase KDF are documented, and any new dependency is justified in the task notes
- [ ] #6 A core test round-trips encrypt then decrypt and checks that a wrong passphrase and a modified byte are rejected
- [ ] #7 The UI states that a lost passphrase cannot be recovered
- [ ] #8 The export action offers plain CSV, an encrypted backup, and restore from an encrypted backup
- [ ] #9 Restoring a backup merges its event log into this device, is idempotent, and re-projects the affected entries and profiles
- [ ] #10 A wrong passphrase, a tampered file, or a conflicting group key fails with a clear error and writes no partial data
- [ ] #11 Tests cover export then import, a repeated import, a wrong passphrase, tampered ciphertext, tampered KDF metadata, and a conflicting group key
- [ ] #12 The UI says that restore merges data and that a lost passphrase cannot be recovered
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add `argon2` (RustCrypto 0.5) to crates/core. Use Argon2id with 64 MiB memory, 3 iterations, 1 lane, a 16-byte salt and a 32-byte key. No other new dependency: reuse XChaCha20-Poly1305, serde_json, and the existing hex helpers.
2. Add `crates/core/src/backup.rs`. The plaintext payload is the event log (`EventLog::all`) plus each topic's group key: format id, version, exported_at, topics (topic + hex key), events (all headers, nonces, payloads, signatures). The file is a text JSON envelope: format, version, KDF params, salt, nonce, ciphertext hex. The AAD is a canonical header string built from those fields, so editing the KDF metadata or nonce makes decryption fail.
3. Export: JSON-encode the payload, derive the key from the passphrase and salt, seal with a random nonce, and serialise the envelope. Decrypt: validate format/version/parameters, derive, open, parse; map wrong key, wrong AAD and tampering to one clear "wrong passphrase or damaged backup" error.
4. Import: decrypt and parse the whole file first. Preflight every event signature and every topic key. Reject a topic whose local group key differs. Then in one SQLite transaction save missing group keys, `EventLog::insert` each event, and `Repo::project` only newly inserted events. Roll back on any error. Return imported/skipped counts.
5. Add a CoreError variant for backup failures so the UI can show a clear message without leaking whether the passphrase or the bytes were wrong.
6. Tauri: add `export_backup(passphrase) -> String` and `import_backup(text, passphrase) -> ImportReport`. Keep file I/O in the webview through the fs plugin so Android content URIs keep working.
7. Capabilities: add `dialog:allow-open` and `fs:allow-read-text-file`. Export reuses the existing `dialog:allow-save` and `fs:allow-write-text-file`.
8. Frontend: add a backup sheet with Export CSV, Export encrypted backup and Restore from backup. Export asks for a passphrase twice; restore opens the file, reads it, asks once, and shows the report. Add the warning that a lost passphrase cannot be recovered and that restore merges. Refresh the store after import.
9. Document the `.llbackup` envelope and Argon2id parameters in the task notes; state that restore merges and that the device key, settings and caregivers are not included.
10. Verify with cargo test, clippy, npm build, the app cargo check, and a manual export then import into a fresh database.
<!-- SECTION:PLAN:END -->
