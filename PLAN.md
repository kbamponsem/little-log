# Little Log — Plan, Data Model, Risks

Open-source, cross-platform baby tracker. Rust core, one codebase, local-first.
UX/visual source of truth: `Little Log.html` (vanilla HTML/CSS/JS, no build step).

---

## 1. Architecture at a glance

```
baby-tracker/
├── Cargo.toml               workspace
├── crates/
│   ├── core/                little-log-core: models, enums, validation,
│   │                        SQLite repository (rusqlite), sync logic, CSV, photos
│   └── server/              little-log-server: axum binary, auth, REST sync,
│                            serves the web UI, uses little-log-core
├── web/                     TypeScript web components (custom elements) + Vite
│   ├── index.html           single `<ll-app>` root
│   ├── src/
│   │   ├── main.ts          entry: styles, component registration, boot/401 hook
│   │   ├── styles/          tokens.css, base.css, index.css (global only)
│   │   ├── core/            component.ts (LLComponent), dom.ts, observable.ts,
│   │   │                    sheet.ts (LLSheet), ui-store.ts (UiStore + event names)
│   │   ├── data/            model.ts, storage.ts (adapters), repository.ts, store.ts
│   │   ├── format.ts        icons.ts  photo.ts
│   │   └── components/
│   │       ├── ll-app.ts    composition root (sign-in screen + screens + tabs + sheets)
│   │       ├── ui/          one file per component, with its own .css
│   │       ├── screens/     ll-auth, ll-today, ll-history, ll-trends, ll-baby
│   │       └── sheets/      feed, diaper, sleep, detail
│   └── public/fonts/        Caprasimo + Jost (OFL), local @font-face
├── app/                     Tauri 2 project (Android first, then iOS/desktop)
│   ├── src-tauri/           Rust commands wrapping little-log-core
│   └── src-tauri/gen/android/  Kotlin: Glance widget, receiver, WorkManager drain,
│                               launcher shortcuts
├── docker/                  multi-stage Dockerfile (cargo-chef), non-root
├── docker-compose.yml       one service, port 8080, ./data:/data, healthcheck
├── .github/workflows/ci.yml cargo test/clippy/fmt, APK, server image
├── README.md  LICENSE (MIT)  PLAN.md
```

**Local-first:** the app owns a SQLite file and works 100 % offline. The server is
an optional sync target; the browser UI (Phase 1) talks to the same REST API the
app syncs against.

**Storage adapter contract** (implemented twice, identical interface):

```js
storage.listEntries()              -> Entry[]
storage.saveEntry(entry)           -> Entry        // upsert, LWW
storage.deleteEntry(id)            -> Entry        // soft delete tombstone
storage.getBaby() / saveBaby(baby)
storage.listCaregivers() / invite(role) / ...
storage.savePhoto(entryId, bytes)  -> path
storage.exportCsv()                -> string
storage.deleteAll()
```

- In the Tauri app: `TauriAdapter` → `invoke('...')` → `little-log-core` (SQLite on device).
- In the browser: `HttpAdapter` → `fetch('/api/...')` → axum → same core.

---

## 2. Data model

### 2.1 Enums (canonical id in storage, label for display)

All enums serialize to stable snake_case ids so sync never depends on
localised text; `label()` renders the exact strings from the prototype.

| Enum | Variants (id → label) |
|---|---|
| `EntryKind` | `feed`→Feed, `sleep`→Sleep, `diaper`→Diaper |
| `FeedKind` | `breast_left`→Breast left, `breast_right`→Breast right, `bottle`→Bottle, `solids`→Solids |
| `DiaperType` | `pee`→Pee, `poop`→Poop, `both`→Both |
| `PeeShade` | `pale`→Pale, `normal`→Normal, `dark`→Dark |
| `Amount` | `small`→Small, `medium`→Medium, `large`→Large |
| `PoopColor` | `mustard`, `yellow`, `green`, `brown`, `dark`, `pale`, `red_streaked` (labels as in prototype, incl. dot colours) |
| `Consistency` | `liquid`, `runny`, `soft`, `pasty`, `firm`, `hard` |
| `SleepLocation` | `crib`, `arms`, `stroller`, `car`, `parents_bed` |
| `WakeMood` | `calm`, `fussy`, `cried` |
| `Role` | `owner`, `co_parent`, `caregiver` |

Unknown strings are rejected (`serde` + explicit validation) — enum validation tests cover this.

### 2.2 SQLite schema (migration v1, `PRAGMA user_version`)

Times are **i64 milliseconds since epoch, UTC**. "Today" is computed in device local time.

```sql
CREATE TABLE babies (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, born TEXT,        -- born = 'YYYY-MM-DD'
  theme TEXT NOT NULL DEFAULT 'auto',                        -- auto|light|dark
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE caregivers (                                    -- users
  id TEXT PRIMARY KEY, baby_id TEXT NOT NULL, name TEXT NOT NULL, email TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,                               -- Argon2id PHC string
  role TEXT NOT NULL,                                        -- owner|co_parent|caregiver
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE entries (
  id TEXT PRIMARY KEY,                                       -- UUIDv7 — idempotent upsert key
  baby_id TEXT NOT NULL,
  kind TEXT NOT NULL,                                        -- feed|sleep|diaper
  occurred_at INTEGER NOT NULL,                              -- start / log time
  ended_at INTEGER,                                          -- sleep end; NULL = sleep in progress
  feed_kind TEXT,                                            -- feed
  amount INTEGER, amount_unit TEXT,                          -- feed: ml (bottle/solids) or min (breast)
  diaper_type TEXT,                                          -- diaper
  pee_shade TEXT, pee_amount TEXT,
  poop_color TEXT, consistency TEXT, poop_amount TEXT,
  sleep_location TEXT, wake_mood TEXT,                       -- sleep
  note TEXT, photo TEXT,                                     -- photo = file name
  caregiver_id TEXT,                                         -- author
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,  -- updated_at = LWW clock
  deleted INTEGER NOT NULL DEFAULT 0                         -- soft delete tombstone
);
CREATE INDEX idx_entries_updated ON entries(updated_at, id);
CREATE INDEX idx_entries_occurred ON entries(kind, occurred_at);

CREATE TABLE sessions (                                      -- server only
  token_hash TEXT PRIMARY KEY,                               -- HMAC-SHA256(SESSION_SECRET, token)
  caregiver_id TEXT NOT NULL, created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL
);

CREATE TABLE invites (
  token_hash TEXT PRIMARY KEY, role TEXT NOT NULL, invited_by TEXT NOT NULL,
  created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, accepted_by TEXT
);
```

### 2.3 Validation rules (core::validate)

- `kind` required; `feed` ⇒ `feed_kind` required.
  - breast ⇒ `amount_unit = "min"`, `amount` 0..=180;
  - bottle ⇒ `amount_unit = "ml"`, `amount` 0..=2000;
  - solids ⇒ optional `amount` + `amount_unit = "ml"`.
- `diaper` ⇒ `diaper_type` required; pee ⇒ shade/amount optional but must be valid
  enum values if present; poop ⇒ same for colour/consistency/amount; `photo` allowed.
- `sleep` ⇒ `ended_at` NULL (running) **or** `ended_at > occurred_at`.
- `occurred_at` not more than 5 min in the future (clock skew allowance).
- `note` ≤ 500 chars; ids must parse as UUID; unknown enum id ⇒ error.

### 2.4 Sync protocol (REST, local-first)

- **Idempotency:** entry ids are UUIDv7 generated on the device before any network
  call; the server upserts by primary key, so retries are safe.
- **Pull:** `GET /api/sync?since=<cursor>` → `{ entries, baby, cursor }`.
  Cursor is composite `"<updated_at>:<entry_id>"` and rows are returned ordered by
  `(updated_at, id)` with strict `>` comparison — no rows lost when two entries
  share a millisecond.
- **Push:** `POST /api/sync` `{ entries: [...] }` → per-row last-write-wins:
  `INSERT … ON CONFLICT(id) DO UPDATE SET … WHERE excluded.updated_at >= entries.updated_at`.
  Re-pushing the same payload is a no-op (idempotent); equal timestamps → incoming wins.
- **Soft deletes:** `deleted = 1` tombstones sync like any update. "Delete all" writes
  tombstones (so other devices converge), then the server may purge tombstones older
  than 30 days.
- **Photos:** `PUT /api/photos/:entry_id` (raw JPEG) / `GET /api/photos/:entry_id`
  (cookie-auth). Stored under `/data/photos/`, resized server-side to max 1280 px JPEG
  (core::photo). The app stores the same file on disk and uploads on sync.
- **Sessions:** server issues a random 256-bit token in an `HttpOnly; SameSite=Lax`
  cookie; only `HMAC-SHA256(SESSION_SECRET, token)` is stored. Argon2id for passwords.

### 2.5 Roles (core::permissions — single source of truth, tested)

| Action | owner | co_parent | caregiver |
|---|---|---|---|
| view entries / baby | ✓ | ✓ | ✓ |
| create entry | ✓ | ✓ | ✓ |
| edit/delete **own** entry | ✓ | ✓ | ✓ |
| edit/delete **any** entry | ✓ | ✓ | ✗ |
| edit baby profile, invite | ✓ | ✓ | ✗ |
| delete all data, remove caregivers | ✓ | ✗ | ✗ |

`can(role, action, is_author) -> bool` is used by axum handlers and tested as a matrix.

---

## 3. Frontend (web/) — TypeScript web components

**Rule: everything is a component.** Every piece of UI is a custom element with
its own stylesheet next to it, so any part can be changed in isolation.

- **Stack:** TypeScript + native custom elements (light DOM, no framework, no
  shadow DOM so the prototype's CSS keeps working) + Vite. `npm install` then
  `npm run dev` (HMR, `/api` proxied to the Rust server) or `npm run build` →
  `dist/` (served by axum, and Tauri's `frontendDist`). Vite/`tsc` replace the
  prototype's "no build step" constraint — the output is still plain
  HTML/CSS/JS.
- **`LLComponent`** (`core/component.ts`): declarative `template()`, `mount()`
  for event wiring, `subscribe()`/`watch(store)` for reactivity, `refresh()`
  skips re-render while the user is typing, `preservesChildren` for wrappers
  like `<ll-card>`.
- **Component tree**
  - `ui/` — primitives, each with a co-located `.css`: `ll-chip`,
    `ll-chip-group`, `ll-tile`, `ll-log-list`, `ll-entry-row`, `ll-field`,
    `ll-stepper`, `ll-photo-picker`, `ll-photo-grid`, `ll-card`, `ll-stat`,
    `ll-bar-chart`, `ll-day-nav`, `ll-caregiver-row`, `ll-button`, `ll-tabs`,
    `ll-sheet-host`. `ui/index.ts` registers them all (and their CSS) at boot.
  - `screens/` — `ll-auth`, `ll-today`, `ll-history`, `ll-trends`, `ll-baby`.
    `ll-auth` is the sign-in front door: a full page (not a sheet) that
    `ll-app` configures from `ui.auth` before unhiding it.
  - `sheets/` — `ll-feed-sheet`, `ll-diaper-sheet`, `ll-sleep-sheet`,
    `ll-detail-sheet`, all built on `LLSheet`
    (`core/sheet.ts`, `field()/error()/close()`; the shared helpers live in
    `core/form.ts`).
  - `ll-app` is the composition root; `main.ts` owns boot + the 401 hook.
- **State split:** `UiStore` (`core/ui-store.ts`) holds view-only state (tab,
  open sheet, whether the sign-in screen has taken over, the boot flag, history
  cursor/filter, invite role) and the inter-component event
  names (`ll-open-entry`, `ll-tile-press`, `ll-chip-press`, `ll-change`,
  `ll-step`, `ll-photo-picked`, `ll-day-change`, `ll-signed-in`). Sheets open and
  close through `ui.openSheet()` / `ui.closeSheet()`; the sign-in screen
  through `ui.showAuth()` / `ui.hideAuth()`. `AppStore` (`data/store.ts`)
  holds data (entries, baby, user) and notifies on change; components re-render
  from it.
- **Storage adapter** (`data/storage.ts`) — identical interface, two
  implementations: `HttpAdapter` (browser → REST, HttpOnly cookie) and
  `TauriAdapter` (app → IPC → `little-log-core`, offline-first). Entry ids are
  UUIDv7 from the `uuid` package; photos are downscaled to ≤1280 px JPEG with
  `compressorjs`.
- **Prototype fidelity kept:** Caprasimo/Jost (bundled locally in
  `public/fonts`), green palette, tiles, bottom sheets, tab bar, feeder cards
  with Breast left default, light/dark.
- **Accessibility deltas vs. prototype:** min 56 px tap targets, one consistent
  `:focus-visible` ring for every control, `prefers-reduced-motion` honoured.
- **Responsive:** one narrow column on phones; at ≥700 px the content column
  widens (and ≥1100 px again), Today's tiles go three-up, Trends pairs its
  cards, the photo gallery gains columns and sheets become centred dialogs.
  The layout tokens (`--content`, `--gutter`, `--tabbar-room`) live in
  `styles/tokens.css`, so breakpoints are declared once. `dvh` units and
  `env(safe-area-inset-*)` keep it correct with mobile keyboards and notches.
- **Templates:** every `template()` is a single ES template literal (no `+`
  concatenation); interpolated values always go through `esc()`.
- **Commands:** `npm run typecheck` (`tsc --noEmit`), `npm run build`
  (`tsc --noEmit && vite build`), `npm run dev`.

## 4. Android widget (highest risk — done first in Phase 2)

**Spike outcome (evaluated `tauri-plugin-widgets` 0.6.2, MIT):**
- It renders Jetpack Glance widgets from a declarative JSON tree held in shared
  preferences — no Kotlin needed, and it would cover the *display* half.
- **Rejected for our one-tap requirement:** its `button`/`link` elements "emit a
  `widget-action` Tauri event back to the app", i.e. the tap is delivered to the
  running app (surfaced to JS via `onWidgetAction`). Tapping to log would need
  the app running/foregrounded, which is exactly what the plan says we must not
  depend on. It also keeps widget config in JS-run shared prefs, so a cold
  process can't render fresh numbers.
- Its docs explicitly bless the fallback: *"If you need custom behavior, create
  your own `GlanceAppWidget`/`GlanceAppWidgetReceiver` and keep the same
  shared-group + config contract."*

**What we built instead (our own Glance widget):**

| Piece | File |
|---|---|
| Queue the receiver appends to (`[{id,action,at}]`, UUIDv7 ids) | `gen/android/.../widget/PendingQueue.kt` |
| One-tap broadcast receiver → queue → WorkManager | `.../widget/LogActionReceiver.kt` |
| Headless drain worker (no activity, no WebView) | `.../widget/WidgetDrainWorker.kt` |
| JNI calls into the core | `.../widget/WidgetBridge.kt` ↔ `app/src-tauri/src/widget.rs` |
| The widget itself: "Last fed 1h 20m ago", "Sleeping 42m", 6 buttons | `.../widget/LittleLogWidget.kt` + `LittleLogWidgetReceiver.kt` |
| Re-render after in-app changes | `.../widget/WidgetUpdater.kt`, `MainActivity.onStop` |
| Launcher shortcuts (same six actions) | `res/xml/shortcuts.xml` + manifest meta-data |
| Queue/drain/state logic (unit-tested) | `crates/core/src/pending.rs`, `app/src-tauri/src/widget.rs` |

- A tap writes a JSON line to `pending_actions.json`; the worker loads the
  cdylib and calls `nativeDrain()`, which applies the taps to SQLite, writes
  `widget_state.json` and returns whether anything was logged. The activity is
  never started.
- **Idempotency:** the tap id *is* the entry id, plus a small applied-ids ledger,
  so a re-delivered broadcast or a retried worker can never double-log.
- The app drains the queue on launch too, and publishes `widget_root.txt` so the
  worker always opens the same database.
- iOS WidgetKit / macOS widgets = Phase 3.

**Still to verify on hardware:** adding the widget, tapping a button with the app
swiped away, WorkManager's background execution, and the shortcut sheet.

**Verified in this environment** (`tauri android build --apk --debug`, no device attached):
- Rust cross-compiles for all four ABIs and the libs are packaged
  (`lib/{arm64-v8a,armeabi-v7a,x86,x86_64}/liblittle_log_app.so`).
- The JNI bridge is real: `nm` on the built lib shows
  `Java_app_littlelog_widget_WidgetBridge_nativeDrain` / `_nativeRefresh`.
- Our Kotlin lands in the APK (`classes7.dex` carries `LittleLogWidget` etc.),
  and the manifest adds the widget provider + `android.app.shortcuts` metadata.
- The web UI is embedded in the native library (Tauri inlines `frontendDist`):
  `strings` finds `index-*.js`, `index-*.css` and `fonts/fonts.css` inside the
  `.so`, matching the freshly built `web/dist`.
- The debug APK is ~633 MB because it keeps four ABIs and unstripped debug
  symbols; release builds (and `--split-per-abi`) are what CI should publish.

Two Glance 1.2 API notes that bit during the spike, now encoded in the code:
`actionRunCallback<ActionCallback>` is the only tap entry point that doesn't
start an activity (there is no `actionStart`), `ColorProvider(day, night)` lives
in `androidx.glance.color` (not `.unit`), and there is no weight modifier — the
buttons use fixed widths.

---

## 5. Risks

| # | Risk | Mitigation |
|---|---|---|
| 1 | **Widget tap-to-log without launching app** (Android background limits, Rust lib load in worker process) | Spike done: `tauri-plugin-widgets` emits taps *into the app*, so we built our own Glance widget + broadcast receiver + WorkManager worker that calls the core over JNI. Unit-tested in Rust; **needs device verification** (add widget, tap with app swiped away) |
| 2 | `tauri-plugin-widgets` may not support custom tap actions | Confirmed by its docs/schema (`widget-action` event to the app) → custom provider, which the plugin's own docs recommend for custom behaviour |
| 3 | No Rust toolchain in env; Android SDK/NDK versions vs Tauri 2 requirements | rustup installed (minimal profile); verify `tauri android dev` early, pin toolchain in `rust-toolchain.toml` |
| 4 | No emulator/device guaranteed in this environment | adb present; if no device, verification falls to CI (APK build) + documented manual step |
| 5 | LWW ties at millisecond granularity lose rows | Composite cursor `ts:id`, `>=` conflict clause, idempotent UUIDv7 upserts |
| 6 | Timezones/DST corrupt "today" stats | Store UTC ms; day boundaries computed in local tz; tests around DST |
| 7 | Photo sync bloat / EXIF privacy | Resize ≤1280 px JPEG in Rust before storage; strip EXIF (re-encode); tombstone cleanup |
| 8 | Cookies/CORS in browser mode | Done: the server serves `web/dist` and the API from one origin, so the `HttpOnly; SameSite=Lax` cookie just works (verified live) |
| 9 | Argon2 login brute force | Done: argon2id at the crate defaults (19 MiB, t=2), unknown emails still hash (timing), and failed sign-ins are throttled per address (tested) |
| 10 | SQLite concurrency (app + worker + sync) | WAL mode, `busy_timeout`, single writer discipline |
| 11 | Fonts offline | Bundle woff2 locally, no runtime network needed |
| 12 | iOS/macOS signing & notarisation (Phase 3) | Documented, out of scope for this session |
| 13 | `SESSION_SECRET` rotation invalidates logins | Documented in README; sessions are re-creatable |

---

## 6. Phases & verification (small steps, each verified)

### Verified: Docker (`docker build` + compose)

- `docker build -f docker/Dockerfile -t little-log-server .` → **156 MB**, four
  stages: web (Vite) → cargo-chef planner → Rust release build → `debian-slim`.
  `ca-certificates` is the only runtime package: the healthcheck is the binary
  itself (`little-log-server healthcheck` does a raw TCP `GET /healthz`, so the
  image carries no curl/wget).
- `docker compose up -d` from the committed files → container healthy,
  `user=littlelog` (uid 10001), `restart=unless-stopped`,
  `./data:/data` mounted, UI + API served on 8080 (`/` 200 html, `/healthz` 200
  json, `/api/entries` 401 without a session).
- Fresh-machine rehearsal on a pristine named volume: `needs_setup:true` →
  first account becomes owner with a baby → a nap logged → CSV export →
  sync pull with cursor; the container runs as uid 10001 and writes
  `little-log.sqlite3` + `photos/` into the volume; **restarting the container
  keeps the data and the session** (stable `SESSION_SECRET`).
- Gotcha worth knowing: Colima only bind-mounts paths under `$HOME`, so the
  pristine test used a Docker named volume instead of a temp directory.

**Phase 1 — core + server + Docker + browser UI** (core + server + Docker done)
- [x] S1 workspace + `little-log-core`: models, enums, validation → `cargo test`
- [x] S2 core: SQLite repository + migrations + CSV + photo → `cargo test`
- [x] S3 core: sync logic (LWW, cursor, tombstones) + widget queue → `cargo test`
- [x] S4 `little-log-server`: axum routes, Argon2id sign-in, invite links, HttpOnly
      cookies, permissions, sync pull/push, CSV, photos, SPA hosting → 17 integration
      tests + a live curl run of the whole flow
- [x] S5 web UI on the storage adapter → `npm run typecheck` + `npm run build`
      (TypeScript web components; the REST adapter lands with S4)
- [x] S6 Dockerfile (cargo-chef, non-root, self-healthcheck) + compose + README
      + MIT licence + `.env.example` + `.gitignore` → image 156 MB, compose up
      healthy, full flow on a pristine volume (see §8)
- [ ] S7 CI workflow + MIT licence → `cargo fmt --check`, `cargo clippy`

**Phase 2 — Android**
- [x] S8 Tauri 2 app (`app/`): core wired through IPC commands, `frontendDist` →
      `../web/dist` → `cargo check`, `cargo clippy`, `tauri android init`
- [x] S9 widget: spike evaluated the plugin, then our own Glance widget +
      `ActionCallback` + WorkManager drain + JNI bridge + launcher shortcuts →
      `tauri android build --apk` succeeds; JNI exports, dex classes, manifest
      metadata and the embedded UI all verified inside the APK (device taps
      still pending — see §4)
- [ ] S10 app↔server sync (push/pull, photos, invites): replaces the `sync_now` stub

**Server: what is left**
- S10 wires the app's `sync_now` command to `/api/sync` (the endpoint is live and tested).
- Docker is in place: `docker/Dockerfile` (web → cargo-chef planner → Rust build →
  debian-slim runtime, non-root uid 10001, `HEALTHCHECK` = `little-log-server
  healthcheck`, which probes `GET /healthz` over raw TCP so the image needs no
  curl), `docker-compose.yml` (one service, 8080, `./data:/data`, healthcheck,
  `restart: unless-stopped`), `.env.example`, `README.md` (setup, backup, HTTPS
  via Caddy or Tailscale) and the MIT licence.
- Still open: tombstone purging (a scheduled job) and CI.

**Phase 3 — later**
- [ ] iOS + macOS desktop targets, WidgetKit widgets
- [ ] Tombstone purging job, rate limiting hardening

**Verification commands per step:** `cargo test`, `cargo clippy --all-targets`,
`cargo fmt --check`, `npm run typecheck && npm run build` (web),
`cargo check` + `cargo clippy` in `app/src-tauri`,
`tauri android build --apk` (CI, no device needed), `tauri android dev` on a
connected device, `docker build -t little-log-server docker/` +
`docker compose up -d` + healthcheck.
