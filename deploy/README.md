# Deploying to Proxmox

One command, after a one-time `.env`:

```bash
cd deploy/terraform
./tf.sh apply
```

That downloads a Debian LXC template, creates the container, installs Docker
inside it, builds the app image there and starts the service. When it finishes,
`output.url` is the address to open.

`tf.sh` exists because Terraform cannot read a `.env` file — it translates yours
into `terraform.tfvars`, which Terraform does auto-load. See
[Your .env](#2-your-env) below.

OpenTofu works identically if you prefer it (`tofu` in place of `terraform`);
just don't swap between them without deleting `.terraform.lock.hcl`, since they
record different provider registries. You do **not** need Docker on your own
machine.

## What gets created

| | |
|---|---|
| LXC container | `debian-13-standard`, unprivileged, nesting + keyctl + fuse |
| Where | whatever `node_name` says, 2 cores / 4 GB / 20 GB disk by default |
| App directory | `/opt/little-log/app` on the container — source, `.env`, `data/` |
| Service | `docker compose`, `restart: unless-stopped`, so it survives reboots |

The image is built **inside** the container on purpose. If your Mac is arm64
and the node is amd64, building locally would run a release Rust compile under
emulation: slow and the usual source of weird failures. Building on the node is
native, and cargo-chef keeps the dependency layers cached for later deploys.

## Before the first apply

### 1. Credentials

Pick one. Both go in a file, never in git.

**API token (recommended — revocable).** On the Proxmox host:

```bash
pveum role add LittleLog --privs "VM.Allocate,VM.Audit,VM.Config.CPU,VM.Config.Disk,VM.Config.Memory,VM.Config.Network,VM.Config.Options,VM.Config.Cloudinit,VM.PowerMgmt,Datastore.AllocateSpace,Datastore.AllocateTemplate,Datastore.Audit,Sys.Audit,Sys.Modify,SDN.Use,SDN.Audit"
pveum user add terraform@pve
pveum aclmod / --users terraform@pve --roles LittleLog
pveum user token add terraform@pve littlelog --privsep=0
```

The last command prints the secret once. If the role already exists,
`pveum role modify LittleLog --privs "…"` updates it instead.

> **`--privsep=0` is not optional.** Proxmox's UI ticks *Privilege Separation*
> by default, which gives the token its own permission set — empty. The token
> then authenticates fine and returns **`{}`** from `/access/permissions`, and
> every call fails with `Permission check failed`. This is by far the most
> common cause of that error.
>
> If you can't recreate the token, grant the role to the token instead of (or
> as well as) the user:
>
> ```bash
> pveum aclmod / --tokens 'terraform@pve!littlelog' --roles LittleLog
> ```
>
> In the UI: Datacenter → Permissions → **Add → API Token Permission**.

`Sys.Audit`, `Sys.Modify` and `Datastore.AllocateTemplate` are called out
specifically by the provider docs as required for the template download —
`Datastore.AllocateTemplate` alone is not enough.

> **Don't reach for the built-in `PVEAdmin` role.** It looks like the right
> answer and it isn't: [the docs](https://pve.proxmox.com/pve-docs/chapter-pveum.html)
> describe it as "can do most tasks, but has no rights to modify system settings
> (**Sys.PowerMgmt, Sys.Modify**, Realm.Allocate)". The template download needs
> `Sys.Modify`, so `PVEAdmin` still fails with `Permission check failed`.
> Use the custom role above — it is `PVEAdmin` plus `Sys.Audit`/`Sys.Modify`/
> `SDN.Audit`, minus the things this deploy never touches.

**Or root, with zero host setup:**

```bash
export PROXMOX_VE_USERNAME='root@pam'
export PROXMOX_VE_PASSWORD='…'
```

Less tidy (a real password in a file), but it needs nothing done on the host
first and has no permission surprises. Good for getting the first deploy done.

### 2. Your .env

The credentials live in `.env`, which stays the single file you edit. It is
already gitignored.

```bash
cd deploy/terraform

cat > .env <<'EOF'
export PROXMOX_VE_ENDPOINT='https://your-proxmox:8006/'
export PROXMOX_VE_API_TOKEN='terraform@pve!littlelog=<secret>'
EOF
chmod 600 .env
```

Add `export PROXMOX_VE_INSECURE=1` only if you talk to the node's IP with its
self-signed certificate. A named address with a real certificate — a Tailscale
name, say — needs no such thing.

> **Careful: Terraform cannot read a `.env` file.** That is a Compose/direnv
> convention; Terraform has no `--env-file`. A variable can only take its value
> from `-var`, a tfvars file, or a `TF_VAR_*` environment variable — it cannot
> be computed from a file. So `terraform apply` right after writing `.env` fails
> with *"Missing Proxmox VE API Endpoint"*, even though the file is right there.

## Apply

`tf.sh` is a two-line bridge: it translates `.env` into `terraform.tfvars`
(which Terraform *does* auto-load) and then runs Terraform.

```bash
cd deploy/terraform
terraform init       # once
./tf.sh plan         # look before you leap
./tf.sh apply
```

Any Terraform subcommand works: `./tf.sh output`, `./tf.sh destroy`, …

`terraform.tfvars` is generated on each run, gitignored, and `chmod 600`
because it holds the token — don't hand-edit it. Use `./tf.sh` rather than a
bare `terraform apply`, or you will deploy stale values from the last sync.

<details>
<summary>Alternatives, if you'd rather not use the wrapper</summary>

**Write the tfvars yourself.** Terraform auto-loads it, so a bare
`terraform apply` works with no shell setup. Copy the template and fill it in:

```bash
cp terraform.tfvars.example terraform.tfvars
```

```hcl
proxmox_endpoint  = "https://your-proxmox:8006/"
proxmox_api_token = "terraform@pve!littlelog=<secret>"
```

**Or source `.env` before each command.** The exports are then in the
environment, and the provider falls back to them whenever the matching tfvars
value is unset:

```bash
set -a; . ./.env; set +a
terraform apply
```

</details>

### First-run timeline

Expect it to sit there for a while. Roughly: template download ~1 min,
container ~30 s, Docker install ~1 min, **image build 5–20 min**. The build
streams its output, so it is not hung.

`plan` does not call the Proxmox API, so a wrong token or a token without
privileges will not surface until `apply`.

Terraform will fail before creating anything if credentials are wrong — you do
not have to worry about half-created state on a bad token.

> **One thing to know:** provisioning logs in as `root` with your key, so
> `terraform.tfstate` holds a copy of `~/.ssh/id_ed25519`. The state file is
> gitignored — treat it like the key itself. If you'd rather it not be there,
> delete `private_key` from the `connection` block in `main.tf` and run
> `ssh-add --apple-use-keychain ~/.ssh/id_ed25519` first instead; the `agent`
> line is already there for that.

## Afterwards

```bash
./tf.sh output url          # http://10.0.0.50:8080
./tf.sh output ssh          # ssh root@10.0.0.50
```

Open that URL — the first account you create becomes the owner.

Once it is built you can hand the build RAM back:

```hcl
# terraform.tfvars
memory_mb = 1024
cores     = 1
disk_size = 8
```

## Updating

Re-upload the source, rebuild and restart:

```bash
./tf.sh apply -var revision=$(date +%s)
```

Without `revision`, applies are idempotent and change nothing. The cached
dependency layers make the second build far quicker than the first.

## HTTPS

The app speaks plain HTTP on :8080 and expects a proxy in front. Caddy and
Tailscale options are in the root [README](../../README.md#https). Once HTTPS
terminates in front of it, set `COOKIE_SECURE=1` in the container's
`.env` and re-deploy.

## Your data

SQLite + photos live in `/opt/little-log/app/data` on the container. Redeploys
wipe the *source* directory but deliberately leave `data/` and `.env` alone, so
the log and everyone's sessions survive.

```bash
ssh root@$(./tf.sh output -raw ipv4) \
  'cd /opt/little-log/app && docker compose stop && tar czf - data' \
  > little-log-$(date +%F).tar.gz
```

**`./tf.sh destroy` deletes the container and its disk, and that includes your
data.** Back up first, or move `data/` onto a Proxmox bind mount.

## Troubleshooting

| Symptom | Cause |
|---|---|
| `Permission check failed` anywhere | Token has no privileges — almost always Privilege Separation left on. See [the note above](#1-credentials), or `pveum aclmod / --tokens '…' --roles LittleLog` |
| `query-url-metadata` / template download 403 | Needs `Sys.Audit` + `Sys.Modify` + `Datastore.AllocateTemplate`, not just the last one |
| `apply` errors on `features` | Unprivileged container flags need `VM.Config.Options`; if still refused, use `root@pam` |
| `No space left on device` during the build | `disk_size` too small; the base images alone are ~2 GB |
| Build killed / `signal: 9` | Out of memory — raise `memory_mb` |
| `apt-get update` fails inside the container | The Debian codename has no Docker repo; set `template_url` to a `debian-12` template |
| `ipv4` output is `null` | DHCP lease not up; `./tf.sh refresh`, or set a static `ipv4_address` |
| `Permission denied (publickey)` | Your key has a passphrase — `ssh-add --apple-use-keychain ~/.ssh/id_ed25519` |
| Everyone got signed out | `SESSION_SECRET` changed — don't delete `.env` on the container |
