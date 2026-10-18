# Little Log

A local-first baby tracker for feeds, sleep and diapers — one Rust codebase for
an Android app with a home-screen widget, iOS/desktop later, and an optional
self-hosted sync server you can run in Docker.

Everything works offline on the device. The server is only there so partners and
grandparents can share the same log.

```
crates/core      models, enums, validation, permissions, SQLite repository,
                 last-write-wins sync, CSV export, photo resizing, widget queue
crates/server    axum binary: REST API + the built web UI (Docker image)
app/             Tauri 2 app (Android first) + the Glance home-screen widget
web/             TypeScript web components (no framework) + Vite build
```

## Quick start (server)

Needs Docker with Compose v2.

```bash
cp .env.example .env
openssl rand -hex 32          # paste into SESSION_SECRET in .env
docker compose up -d
```

Open <http://localhost:8080>. The first account you create becomes the owner;
the server tells the UI to offer "create account" until then. Everyone else
joins through an invite link you generate in **Baby → Caregivers**.

```bash
docker compose logs -f          # follow logs
docker compose ps               # health: healthy
docker compose down             # stop (data stays in ./data)
docker compose pull && docker compose up -d --build   # update
```

## HTTPS

The server speaks plain HTTP on port 8080 and expects a proxy in front of it.
Both options below keep the session cookie `Secure` when you set
`COOKIE_SECURE=1` in `.env`.

**Caddy** (automatic Let's Encrypt certificates)

```caddyfile
baby.example.com {
    reverse_proxy localhost:8080
}
```

```bash
caddy run --config Caddyfile
```

**Tailscale** (private, no public DNS needed)

```bash
tailscale serve --bg 8080        # https://<machine>.<tailnet>.ts.net
# then set COOKIE_SECURE=1 in .env and docker compose up -d
```

Either way, behind HTTPS set:

```env
COOKIE_SECURE=1
```

## Backup

Every byte of state lives in `./data`:

```
data/little-log.sqlite3        all entries, the baby profile, caregivers
data/little-log.sqlite3-wal    write-ahead log (SQLite)
data/photos/<entry id>.jpg     diaper photos, already downscaled
```

SQLite is safe to copy only while it is not being written, so stop the container
first:

```bash
docker compose stop
tar czf little-log-backup-$(date +%F).tar.gz data/
docker compose start
```

Restore by unpacking over `./data` with the container stopped. The CSV export
(**Baby → Your data → Copy as CSV**) is a handy human-readable copy, but it is
not a full backup: it has no photos.

## Deploying to a home server

`deploy/` has a Terraform/OpenTofu setup that provisions a Debian LXC on a
Proxmox node and deploys the app in one step — template download, container,
Docker, image build and start:

```bash
cd deploy/terraform
cat > .env <<'EOF'          # once — this is the file you keep editing
export PROXMOX_VE_ENDPOINT='https://your-proxmox:8006/'
export PROXMOX_VE_API_TOKEN='terraform@pve!littlelog=<secret>'
EOF
./tf.sh apply
```

No Docker needed on your own machine: the image is built on the container.
See [deploy/README.md](deploy/README.md) for the credentials, the first-run
timeline, updates (`./tf.sh apply -var revision=$(date +%s)`), HTTPS and
backups.

## Running it for development

```bash
# server + API (REST) on :8080
cargo test --workspace
DATA_DIR=./data WEB_DIR=web/dist cargo run -p little-log-server

# web UI with hot reload on :5173, /api proxied to :8080
cd web && npm install && npm run dev

# phone app (needs the Android SDK/NDK and a device or emulator):
#   export ANDROID_HOME=~/Android NDK_HOME=$ANDROID_HOME/ndk/<version>
cd app && npm install
npm run android:dev                       # or: npm run android:build -- --apk
npm run dev                               # desktop window
```

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `PORT` | `8080` | Listen port (inside the container this stays 8080) |
| `DATA_DIR` | `/data` | SQLite file + `photos/` |
| `WEB_DIR` | `./web/dist` | Built frontend to serve; missing → API only |
| `SESSION_SECRET` | *(random)* | HMAC key for sessions and invite links (≥16 chars). Set it, or everyone is signed out on restart |
| `COOKIE_SECURE` | `0` | `1` marks the session cookie `Secure` (use behind HTTPS) |
| `RUST_LOG` | `info` | `error`…`trace` |

## What the server does

- **Accounts:** Argon2id passwords, 256-bit session tokens stored only as
  `HMAC-SHA256(SESSION_SECRET, token)` in an `HttpOnly; SameSite=Lax` cookie.
- **Roles:** owner (everything), co-parent (edit all + invite), caregiver (view
  and add, plus editing their own rows). Enforced in one shared matrix.
- **Sync:** `GET /api/sync?since=<cursor>` and `POST /api/sync`, last-write-wins
  on `updated_at`, soft deletes as tombstones, device-generated UUIDv7 entry ids
  so retries are idempotent.
- **Photos:** `PUT /api/photos/<entry id>` accepts a JPEG/PNG (≤12 MB), resizes
  it to ≤1280 px and re-encodes it as JPEG (EXIF, including GPS, is dropped).
- **Export:** `GET /api/export.csv` — the same `time,type,what,details,notes`
  columns the UI offers.
- **Health:** `GET /healthz`, which also touches the database.

## API map

```
GET    /healthz
GET    /api/auth/status          POST /api/auth/register   POST /api/auth/login
GET    /api/auth/me              POST /api/auth/logout
GET    /api/entries              POST /api/entries         DELETE /api/entries/{id}
GET    /api/sync?since=…         POST /api/sync
GET    /api/baby                 PUT  /api/baby
GET    /api/caregivers           POST /api/invites
PUT    /api/photos/{id}          GET  /api/photos/{id}
GET    /api/export.csv           DELETE /api/data
```

## License

MIT — see [LICENSE](LICENSE). Bundled fonts (Caprasimo, Jost) are under the SIL
Open Font License; see `web/public/fonts/`.
