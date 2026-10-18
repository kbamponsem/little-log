# Terraform for Little Log on Proxmox.
#
# One command end to end: `tofu apply` (or `terraform apply`) downloads a
# Debian LXC template, creates the container, installs Docker inside it, builds
# the app image there and starts the service.
#
# The app image is built *inside* the container on purpose. This machine may be
# an arm64 Mac while the Proxmox node is amd64, and cross-building a release
# Rust binary under emulation is slow and fragile — so nothing here needs
# Docker on your own machine.

terraform {
  required_version = ">= 1.6"

  required_providers {
    proxmox = {
      # Pinned to a minor line on purpose: bpg is pre-1.0 and does rename
      # things. Bump deliberately, then re-run `init`.
      source  = "bpg/proxmox"
      version = "~> 0.117"
    }
  }
}

provider "proxmox" {
  # These come from terraform.tfvars (loaded automatically — so a bare
  # `terraform apply` works with no shell setup). Left unset, they fall back to
  # the matching environment variables, so either style works:
  #
  #   terraform.tfvars                environment variables
  #   ────────────────                ─────────────────────
  #   proxmox_endpoint                PROXMOX_VE_ENDPOINT
  #   proxmox_api_token               PROXMOX_VE_API_TOKEN
  #   proxmox_insecure                PROXMOX_VE_INSECURE
  #
  # There is deliberately no `ssh` block. The provider only needs SSH for
  # snippet uploads, container `idmap` entries and disk imports — none of which
  # this configuration uses — so it never opens an SSH session to the node.
  endpoint  = var.proxmox_endpoint
  api_token = var.proxmox_api_token
  insecure  = var.proxmox_insecure
}

# The LXC template, fetched by the node itself through the PVE download-url API.
resource "proxmox_download_file" "debian" {
  content_type   = "vztmpl"
  datastore_id   = var.template_datastore_id
  node_name      = var.node_name
  url            = var.template_url
  upload_timeout = var.template_upload_timeout

  # Never silently re-download a template that is already there.
  overwrite = false
  # But do take ownership of one you already grabbed from the Proxmox UI,
  # instead of failing because the filename is taken.
  overwrite_unmanaged = true
}

resource "proxmox_virtual_environment_container" "little_log" {
  node_name   = var.node_name
  vm_id       = var.vm_id
  description = "Little Log — self-hosted baby tracker (managed by Terraform)"

  # Unprivileged is the safe default.
  unprivileged = true

  # Only `nesting` is settable with an API token: PVE rejects keyctl and fuse
  # with "changing feature flags (except nesting) is only allowed for root@pam",
  # and that is not a privilege you can grant. Docker runs with nesting alone,
  # which is also why the container uses overlay2 as its storage driver.
  features {
    nesting = true
  }

  start_on_boot = true

  cpu {
    cores = var.cores
  }

  memory {
    # The Rust release build is the peak consumer. Lower this after the first
    # successful deploy if you want the RAM back — see the README.
    dedicated = var.memory_mb
  }

  disk {
    datastore_id = var.datastore_id
    size         = var.disk_size
  }

  network_interface {
    name   = "eth0"
    bridge = var.bridge
  }

  initialization {
    hostname = var.hostname

    ip_config {
      ipv4 {
        address = var.ipv4_address
        # Must be omitted when using DHCP.
        gateway = var.ipv4_address == "dhcp" ? null : var.ipv4_gateway
      }
    }

    # Authorises your key for root, which is what the provisioning steps below
    # log in with.
    user_account {
      keys = [trimspace(file(pathexpand(var.ssh_public_key_path)))]
    }
  }

  operating_system {
    template_file_id = proxmox_download_file.debian.id
    type             = "debian"
  }

  # With DHCP the address has to be known before we can ssh in and provision.
  wait_for_ip {
    ipv4 = true
  }
}

locals {
  container_ip = values(proxmox_virtual_environment_container.little_log.ipv4)[0]
}

# Everything that happens *inside* the container.
#
# `terraform_data` is just a hook for provisioners — it holds no remote state.
# Re-run the deploy at any time with:
#
#   tofu apply -var revision=$(date +%s)
resource "terraform_data" "provision" {
  triggers_replace = {
    revision  = var.revision
    container = proxmox_virtual_environment_container.little_log.id
  }

  connection {
    type  = "ssh"
    host  = local.container_ip
    user  = "root"
    agent = true
    # Falls back to the key file when the agent has no identities loaded.
    private_key = file(pathexpand(var.ssh_private_key_path))
    timeout     = "15m"
  }

  # Ship the source the image needs. `app/` is excluded because the server image
  # is Rust + web only — that keeps the upload a few MB instead of dragging
  # build output along with it. `.env` is excluded so a local secret can never
  # overwrite the container's own.
  provisioner "local-exec" {
    interpreter = ["/bin/bash", "-c"]
    environment = { CONTAINER_IP = local.container_ip }
    command     = <<-EOT
      set -euo pipefail
      cd "${path.module}/../.."

      # Runs on the container. Wipes the previous source but deliberately
      # leaves `data/` (the SQLite database and photos) and `.env` alone.
      REMOTE='
      set -euo pipefail
      mkdir -p /opt/little-log/app
      cd /opt/little-log/app
      find . -mindepth 1 -maxdepth 1 ! -name data ! -name .env -exec rm -rf {} +
      tar xzf - -C /opt/little-log/app
      '

      tar czf - \
        --exclude=./target \
        --exclude=./app \
        --exclude=./data \
        --exclude=./.env \
        --exclude=./web/node_modules \
        --exclude=./web/dist \
        --exclude=./.git \
        --exclude=./deploy/terraform/.terraform \
        --exclude='*.tfstate*' \
        . | ssh -o BatchMode=yes -o ConnectTimeout=10 \
                -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=/dev/null \
                "root@$CONTAINER_IP" "$REMOTE"
    EOT
  }

  # Docker install, .env, build, start, health check. Long first run: it is
  # compiling the Rust binary natively on the node.
  provisioner "remote-exec" {
    inline = ["bash /opt/little-log/app/deploy/provision.sh"]
  }
}

output "url" {
  description = "Where the app is serving."
  value       = "http://${local.container_ip}:8080"
}

output "ipv4" {
  description = "The container's IPv4 address."
  value       = local.container_ip
}

output "ssh" {
  description = "Shell into the container."
  value       = "ssh root@${local.container_ip}"
}
