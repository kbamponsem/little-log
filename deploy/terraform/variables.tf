# Every knob for the Proxmox LXC. The credentials below are the only ones you
# normally need to set, and they go in terraform.tfvars (gitignored), which
# Terraform loads automatically — see terraform.tfvars.example.

variable "proxmox_endpoint" {
  description = "Proxmox API URL, including the port unless you front it with Tailscale or a proxy. Falls back to PROXMOX_VE_ENDPOINT when unset."
  type        = string
  default     = null

  validation {
    condition     = var.proxmox_endpoint == null || can(regex("^https?://[^/]+/?$", var.proxmox_endpoint))
    error_message = "Must be a URL like https://host:8006/ — with the scheme, and no path or trailing double slash."
  }
}

variable "proxmox_api_token" {
  description = "API token as USER@REALM!TOKENID=SECRET. Falls back to PROXMOX_VE_API_TOKEN when unset."
  type        = string
  default     = null
  sensitive   = true

  validation {
    condition     = var.proxmox_api_token == null || can(regex("^[^@!]+@[^!]+![^=]+=.+$", var.proxmox_api_token))
    error_message = "Needs the FULL token: USER@REALM!TOKENID=SECRET. That is easy to get wrong, because the Proxmox UI shows the two halves in separate columns and only shows the secret once. `pveum user token add ...` prints the whole thing."
  }
}

variable "proxmox_insecure" {
  description = "Skip TLS verification. Only needed for a self-signed certificate, i.e. talking to the node's IP rather than a named address. Falls back to PROXMOX_VE_INSECURE."
  type        = bool
  default     = null
}

variable "node_name" {
  description = "The Proxmox node to create the container on. `pvesh get /nodes` lists the names."
  type        = string
  default     = "pve"
}

variable "vm_id" {
  description = "Container ID. Must be free on the node; change it if apply says the ID is taken."
  type        = number
  default     = 120
}

variable "hostname" {
  description = "Container hostname."
  type        = string
  default     = "little-log"
}

variable "template_url" {
  description = <<-EOT
    The LXC template image, downloaded by the node. Defaults to the current
    Debian standard template for amd64. Browse
    https://download.proxmox.com/images/system/ for others, and switch the
    filename to arm64 if the node is ARM.
  EOT
  type        = string
  default     = "https://download.proxmox.com/images/system/debian-13-standard_13.6-1_amd64.tar.zst"
}

variable "template_datastore_id" {
  description = "Storage pool to save the template on. Must allow the `vztmpl` content type (the default `local` does)."
  type        = string
  default     = "local"
}

variable "template_upload_timeout" {
  description = "Seconds to allow for the template download (~130 MB)."
  type        = number
  default     = 900
}

variable "datastore_id" {
  description = "Storage pool for the container's root filesystem."
  type        = string
  default     = "local-lvm"
}

variable "disk_size" {
  description = <<-EOT
    Root filesystem size in GB. This has to hold the base images (rust + node,
    about 2 GB), the Rust build output and the Docker layer cache, so it wants
    to be roomier than the app's actual size suggests.
  EOT
  type        = number
  default     = 20
}

variable "cores" {
  description = "CPU cores. More cores means a faster first build."
  type        = number
  default     = 2
}

variable "memory_mb" {
  description = "Dedicated memory in MB. 4 GB covers the Rust release build; 1 GB is plenty once built."
  type        = number
  default     = 4096
}

variable "bridge" {
  description = "Network bridge to attach to."
  type        = string
  default     = "vmbr0"
}

variable "ipv4_address" {
  description = "Container IPv4 in CIDR form, or \"dhcp\". The scripts read whatever the provider reports, so DHCP needs no extra configuration."
  type        = string
  default     = "dhcp"
}

variable "ipv4_gateway" {
  description = "IPv4 gateway. Only used when ipv4_address is not \"dhcp\"."
  type        = string
  default     = ""
}

variable "ssh_public_key_path" {
  description = "Public key authorised for root inside the container."
  type        = string
  default     = "~/.ssh/id_ed25519.pub"
}

variable "ssh_private_key_path" {
  description = "Matching private key, used to provision the container."
  type        = string
  default     = "~/.ssh/id_ed25519"
}

variable "revision" {
  description = <<-EOT
    Bump this to re-run the deploy (re-upload the source, rebuild, restart):
      tofu apply -var revision=$(date +%s)
    Left empty, applies are idempotent and touching nothing.
  EOT
  type        = string
  default     = ""
}
