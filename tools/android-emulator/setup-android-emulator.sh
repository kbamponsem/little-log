#!/usr/bin/env bash
#
# setup-android-emulator.sh
#
# Installs and configures an Android emulator on macOS with no manual steps,
# then prepares the Rust + NDK toolchain for a Tauri 2 Android app.
#
# Written for the macOS default toolchain: bash 3.2 and the BSD userland.
#
# Usage: setup-android-emulator.sh [options]
#   --dry-run         print what would happen, change nothing
#   --avd-name NAME   AVD name to create (default: little-log-pixel)
#   --skip-rust       skip the Rust targets + cargo-ndk step
#   --no-start        do not start the emulator
#   --uninstall       remove only what this script installed (asks first)
#   -h, --help        show this help
#
set -euo pipefail

SCRIPT_NAME="${0##*/}"

# --- configuration -----------------------------------------------------------

SDK_ROOT="${ANDROID_SDK_DIR:-$HOME/Library/Android/sdk}"
AVD_NAME="little-log-pixel"
BOOT_TIMEOUT="${BOOT_TIMEOUT:-300}"
MIN_FREE_GB=15

STATE_DIR="$HOME/.little-log-android-setup"
STATE_FILE="$STATE_DIR/manifest"
PROFILE_BEGIN="# >>> little-log android setup >>>"
PROFILE_END="# <<< little-log android setup <<<"

RUST_TARGETS="aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android"

DRY_RUN=0
SKIP_RUST=0
NO_START=0
UNINSTALL=0

ARCH=""
ABI=""
CMDLINE_ARCH=""
PLATFORM_PKG=""
BUILD_TOOLS_PKG=""
NDK_PKG=""
IMAGE_PKG=""
NDK_HOME_DIR=""
JAVA_HOME_DIR=""
PROFILE_FILE=""
DEVICE_PROFILE=""
SCRATCH=""
SDKMANAGER=""
AVDMANAGER=""
ADB=""
TARGET_SERIAL=""

# --- output helpers ----------------------------------------------------------

info() { printf '       %s\n' "$*"; }
ok()   { printf '[ok]   %s\n' "$*"; }
skip() { printf '[skip] %s\n' "$*"; }
warn() { printf '[warn] %s\n' "$*" >&2; }
fail() { printf '[fail] %s\n' "$*" >&2; exit 1; }
step() { printf '\n== %s\n' "$*"; }

have() { command -v "$1" >/dev/null 2>&1; }

# Run a mutating command, or just describe it under --dry-run.
run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[--]   would run: %s\n' "$*"
    return 0
  fi
  "$@"
}

# Remember something we installed, so --uninstall can undo exactly that.
record() {
  if [ "$DRY_RUN" -eq 1 ]; then
    return 0
  fi
  mkdir -p "$STATE_DIR"
  if [ -f "$STATE_FILE" ] && grep -qxF -- "$*" "$STATE_FILE"; then
    return 0
  fi
  printf '%s\n' "$*" >>"$STATE_FILE"
}

cleanup() {
  if [ -n "$SCRATCH" ] && [ -d "$SCRATCH" ]; then
    rm -rf "$SCRATCH"
  fi
}
trap cleanup EXIT

usage() {
  cat <<EOF
${SCRIPT_NAME}

Installs and configures an Android emulator on macOS with no manual steps,
then prepares the Rust + NDK toolchain for a Tauri 2 Android app.

Usage: ${SCRIPT_NAME} [options]
  --dry-run         print what would happen, change nothing
  --avd-name NAME   AVD name to create (default: little-log-pixel)
  --skip-rust       skip the Rust targets + cargo-ndk step
  --no-start        do not start the emulator
  --uninstall       remove only what this script installed (asks first)
  -h, --help        show this help
EOF
}

# --- argument parsing --------------------------------------------------------

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --dry-run)     DRY_RUN=1 ;;
      --skip-rust)   SKIP_RUST=1 ;;
      --no-start)    NO_START=1 ;;
      --uninstall)   UNINSTALL=1 ;;
      --avd-name)    shift; [ $# -gt 0 ] || fail "--avd-name needs a value"; AVD_NAME="$1" ;;
      --avd-name=*)  AVD_NAME="${1#*=}" ;;
      -h|--help)     usage; exit 0 ;;
      *)             fail "unknown argument: $1 (try --help)" ;;
    esac
    shift
  done
}

# --- preflight ---------------------------------------------------------------

preflight() {
  step "Preflight"

  [ "$(uname -s)" = "Darwin" ] || fail "this script supports macOS only (found $(uname -s))."
  ok "macOS detected"

  if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
    fail "this terminal is running under Rosetta on Apple Silicon.
       Open a native arm64 terminal (or run 'arch -arm64 \$SHELL') and try again."
  fi

  case "$(uname -m)" in
    arm64|aarch64) ARCH="arm64";   ABI="arm64-v8a"; CMDLINE_ARCH="mac_arm64" ;;
    x86_64)        ARCH="x86_64";  ABI="x86_64";    CMDLINE_ARCH="mac_x86_64" ;;
    *)             fail "unsupported CPU: $(uname -m)" ;;
  esac
  ok "CPU ${ARCH} -> system image ABI ${ABI}"

  if xcode-select -p >/dev/null 2>&1; then
    ok "Xcode command line tools: $(xcode-select -p)"
  elif [ "$DRY_RUN" -eq 1 ]; then
    info "would run: xcode-select --install"
  else
    warn "Xcode command line tools are not installed."
    reply="n"
    read -r -p "Run 'xcode-select --install' now? [y/N] " reply || reply="n"
    if [ "${reply#y}" != "$reply" ] || [ "${reply#Y}" != "$reply" ]; then
      xcode-select --install || true
      fail "re-run this script once the command line tools finish installing."
    fi
    fail "Xcode command line tools are required (run: xcode-select --install)."
  fi

  if curl -fsS --max-time 15 -o /dev/null https://dl.google.com/android/repository/repository2-3.xml; then
    ok "internet reachable"
  else
    fail "no internet access (could not reach dl.google.com)."
  fi

  free_gb="$(df -g "$HOME" | awk 'NR==2 {print $4}')"
  if [ -z "$free_gb" ] || [ "$free_gb" -lt "$MIN_FREE_GB" ]; then
    fail "need at least ${MIN_FREE_GB} GB free in \$HOME (found ${free_gb:-unknown} GB)."
  fi
  ok "${free_gb} GB free in \$HOME"
}

# --- toolchain bootstrap -----------------------------------------------------

ensure_brew() {
  step "Homebrew"
  if have brew; then
    ok "Homebrew already installed ($(brew --version | head -1))"
    return 0
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    info "would install Homebrew from https://brew.sh"
    return 0
  fi
  NONINTERACTIVE=1 /bin/bash -c \
    "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
  record "brew:homebrew"
  if [ -x /opt/homebrew/bin/brew ]; then
    eval "$(/opt/homebrew/bin/brew shellenv)"
  elif [ -x /usr/local/bin/brew ]; then
    eval "$(/usr/local/bin/brew shellenv)"
  fi
  have brew || fail "Homebrew installation failed."
  ok "Homebrew installed"
}

ensure_java() {
  step "JDK 17 (what Tauri 2 and the Android Gradle plugin expect)"
  p=""
  for p in /opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home \
           /usr/local/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home; do
    if [ -x "$p/bin/javac" ]; then
      JAVA_HOME_DIR="$p"
      export JAVA_HOME="$p"
      ok "JDK 17 already installed: $p"
      return 0
    fi
  done

  if [ -n "${JAVA_HOME:-}" ] && [ -x "${JAVA_HOME}/bin/javac" ]; then
    JAVA_HOME_DIR="$JAVA_HOME"
    ok "reusing existing JAVA_HOME: $JAVA_HOME"
    return 0
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    JAVA_HOME_DIR="/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home"
    info "would install openjdk@17 with Homebrew"
    return 0
  fi

  run brew install openjdk@17
  record "brew:openjdk@17"
  JAVA_HOME_DIR="$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home"
  [ -x "$JAVA_HOME_DIR/bin/javac" ] || fail "openjdk@17 installed but javac was not found."
  export JAVA_HOME="$JAVA_HOME_DIR"
  ok "JDK 17 installed: $JAVA_HOME_DIR"
}

ensure_rustup() {
  step "rustup"
  if have rustup; then
    ok "rustup already installed ($(rustup --version 2>/dev/null | head -1))"
    return 0
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    info "would install rustup with Homebrew"
    return 0
  fi
  run brew install rustup-init
  record "brew:rustup-init"
  run rustup-init -y --no-modify-path --default-toolchain stable --profile minimal
  export PATH="$HOME/.cargo/bin:$PATH"
  have rustup || fail "rustup installation failed."
  ok "rustup installed"
}

ensure_cmdline_tools() {
  step "Android command-line tools"
  if [ -x "$SDKMANAGER" ]; then
    ok "sdkmanager already present"
    return 0
  fi

  # Look the download up rather than hardcoding a build number.
  url="$(curl -fsS --max-time 60 https://developer.android.com/studio \
        | grep -o "commandlinetools-${CMDLINE_ARCH}-[0-9]*_latest.zip" \
        | sort -u | tail -1 || true)"
  [ -n "$url" ] || fail "could not find a ${CMDLINE_ARCH} cmdline-tools download on developer.android.com."
  ok "latest cmdline-tools: ${url}"

  if [ "$DRY_RUN" -eq 1 ]; then
    info "would unpack it to ${SDK_ROOT}/cmdline-tools/latest"
    return 0
  fi

  if [ ! -d "$SDK_ROOT" ]; then
    record "sdk-root-created:$SDK_ROOT"
  fi
  SCRATCH="$(mktemp -d)"
  curl -fsSL --max-time 900 -o "$SCRATCH/cmdline-tools.zip" \
    "https://dl.google.com/android/repository/${url}"
  unzip -q "$SCRATCH/cmdline-tools.zip" -d "$SCRATCH"
  mkdir -p "$SDK_ROOT/cmdline-tools"
  if [ -d "$SDK_ROOT/cmdline-tools/latest" ]; then
    rm -rf "$SDK_ROOT/cmdline-tools/latest"
  fi
  mv "$SCRATCH/cmdline-tools" "$SDK_ROOT/cmdline-tools/latest"
  rm -rf "$SCRATCH"
  SCRATCH=""
  record "sdk:$SDK_ROOT"
  ok "cmdline-tools installed at ${SDK_ROOT}/cmdline-tools/latest"
}

accept_licenses() {
  step "Android SDK licences"
  if [ "$DRY_RUN" -eq 1 ]; then
    info "would run: yes | sdkmanager --licenses"
    return 0
  fi
  if [ ! -x "$SDKMANAGER" ]; then
    info "sdkmanager not installed; skipped"
    return 0
  fi
  yes | "$SDKMANAGER" --sdk_root="$SDK_ROOT" --licenses >/dev/null 2>&1 || true
  ok "all licences accepted non-interactively"
}

# --- package resolution (no hardcoded versions) ------------------------------

sdk_list() {
  "$SDKMANAGER" --sdk_root="$SDK_ROOT" --list 2>/dev/null
}

# Highest stable package id matching an ERE; previews (rc/beta/alpha/dev) are
# filtered out by reading the version column, not the id.
sdk_pick() {
  sdk_list | awk -F'|' -v re="$1" '
    {
      id = $1; ver = $2
      gsub(/^[ \t]+|[ \t]+$/, "", id)
      gsub(/^[ \t]+|[ \t]+$/, "", ver)
    }
    id ~ re && ver !~ /(rc|beta|alpha|dev)/ { print id }
  ' | sort -u -V | tail -1
}

# Latest stable NDK at r28 or newer (16 KB page alignment).
ndk_pick() {
  sdk_list | awk -F'|' '
    {
      id = $1; ver = $2
      gsub(/^[ \t]+|[ \t]+$/, "", id)
      gsub(/^[ \t]+|[ \t]+$/, "", ver)
      n = split(id, parts, ";")
      if (n == 2 && parts[1] == "ndk" && ver !~ /(rc|beta|alpha|dev)/) {
        split(parts[2], v, ".")
        if (v[1] + 0 >= 28) print id
      }
    }
  ' | sort -u -V | tail -1
}

resolve_packages() {
  step "Resolving latest stable packages"

  if [ ! -x "$SDKMANAGER" ]; then
    PLATFORM_PKG="platforms;android-<latest>"
    BUILD_TOOLS_PKG="build-tools;<latest>"
    NDK_PKG="ndk;<r28+>"
    IMAGE_PKG="system-images;android-<latest>;google_apis_playstore;${ABI}"
    info "sdkmanager not installed yet; package ids resolve during install"
    return 0
  fi

  PLATFORM_PKG="$(sdk_pick '^platforms;android-[0-9]+(\.[0-9]+)?$')"
  [ -n "$PLATFORM_PKG" ] || fail "no stable platform found via sdkmanager --list."
  api_major="${PLATFORM_PKG##*android-}"
  api_major="${api_major%%.*}"
  ok "platform: ${PLATFORM_PKG} (API ${api_major})"

  BUILD_TOOLS_PKG="$(sdk_pick "^build-tools;${api_major}\\.")"
  if [ -z "$BUILD_TOOLS_PKG" ]; then
    BUILD_TOOLS_PKG="$(sdk_pick '^build-tools;')"
  fi
  [ -n "$BUILD_TOOLS_PKG" ] || fail "no stable build-tools found via sdkmanager --list."
  ok "build-tools: ${BUILD_TOOLS_PKG}"

  NDK_PKG="$(ndk_pick)"
  [ -n "$NDK_PKG" ] || fail "no stable NDK >= r28 found via sdkmanager --list."
  ok "NDK: ${NDK_PKG}"

  IMAGE_PKG="$(sdk_pick "^system-images;android-${api_major};google_apis_playstore;${ABI}$")"
  if [ -z "$IMAGE_PKG" ]; then
    IMAGE_PKG="$(sdk_pick "^system-images;android-[0-9]+;google_apis_playstore;${ABI}$")"
  fi
  [ -n "$IMAGE_PKG" ] || fail "no google_apis_playstore system image for ${ABI}."
  ok "system image: ${IMAGE_PKG}"
}

install_packages() {
  step "Installing SDK packages"
  if [ "$DRY_RUN" -eq 1 ]; then
    info "would install: platform-tools emulator ${PLATFORM_PKG} ${BUILD_TOOLS_PKG} ${NDK_PKG} ${IMAGE_PKG}"
    return 0
  fi
  if [ ! -x "$SDKMANAGER" ]; then
    info "sdkmanager unavailable; skipped"
    return 0
  fi

  pkgs=(platform-tools emulator "$PLATFORM_PKG" "$BUILD_TOOLS_PKG" "$NDK_PKG" "$IMAGE_PKG")
  run "$SDKMANAGER" --sdk_root="$SDK_ROOT" "${pkgs[@]}"

  if [ "$DRY_RUN" -eq 0 ]; then
    for pkg in "${pkgs[@]}"; do
      record "sdk-pkg:$pkg"
    done
  fi

  NDK_HOME_DIR="${SDK_ROOT}/ndk/${NDK_PKG#ndk;}"
  export NDK_HOME="$NDK_HOME_DIR"
  ok "installed: ${pkgs[*]}"
}

# --- AVD ---------------------------------------------------------------------

pick_device_profile() {
  for cand in pixel_9 pixel_9_pro pixel_9a pixel_8a pixel_8 pixel_7; do
    if "$AVDMANAGER" list device 2>/dev/null | grep -q "\"${cand}\""; then
      printf '%s' "$cand"
      return 0
    fi
  done
  printf '%s' ""
}

set_avd_config() {
  # $1 file, $2 key, $3 value
  if grep -q "^$2=" "$1" 2>/dev/null; then
    sed -i '' "s|^$2=.*|$2=$3|" "$1"
  else
    printf '%s=%s\n' "$2" "$3" >>"$1"
  fi
}

ensure_avd() {
  step "Android Virtual Device"
  if [ "$DRY_RUN" -eq 1 ]; then
    info "would create AVD '${AVD_NAME}' from ${IMAGE_PKG}"
    info "would set hw.ramSize=4096, hw.keyboard=yes, hw.gpu.mode=host"
    return 0
  fi
  if [ ! -x "$AVDMANAGER" ]; then
    info "avdmanager unavailable; skipped"
    return 0
  fi

  if "$AVDMANAGER" list avd 2>/dev/null | grep -q "Name: ${AVD_NAME}$"; then
    skip "AVD '${AVD_NAME}' already exists"
    return 0
  fi

  avd_ini="$HOME/.android/avd/${AVD_NAME}.avd/config.ini"
  create_log="$(mktemp)"

  DEVICE_PROFILE="$(pick_device_profile)"
  if [ -n "$DEVICE_PROFILE" ]; then
    ok "device profile: ${DEVICE_PROFILE}"
    set -- create avd -n "$AVD_NAME" -k "$IMAGE_PKG" -d "$DEVICE_PROFILE"
  else
    warn "no recent Pixel profile found; creating with the default device"
    set -- create avd -n "$AVD_NAME" -k "$IMAGE_PKG"
  fi

  # avdmanager writes noise (a Play image ships no devices.xml) even on success,
  # so judge the result by the config file it produces, not by its exit code.
  echo no | "$AVDMANAGER" "$@" >"$create_log" 2>&1 || true
  if [ ! -f "$avd_ini" ]; then
    sed 's/^/       /' "$create_log" >&2
    rm -f "$create_log"
    fail "avdmanager could not create '${AVD_NAME}'."
  fi
  if grep -q "Could not load devices" "$create_log"; then
    info "note: avdmanager warned about a missing devices.xml (harmless for Play images)"
  fi
  rm -f "$create_log"

  set_avd_config "$avd_ini" hw.ramSize 4096
  set_avd_config "$avd_ini" hw.keyboard yes
  set_avd_config "$avd_ini" hw.gpu.enabled yes
  set_avd_config "$avd_ini" hw.gpu.mode host
  record "avd:${AVD_NAME}"
  ok "AVD '${AVD_NAME}' created (4 GB RAM, hardware keyboard, host GPU)"
}

# --- Rust + NDK --------------------------------------------------------------

setup_rust() {
  step "Rust Android toolchain"
  if [ "$SKIP_RUST" -eq 1 ]; then
    skip "--skip-rust given"
    return 0
  fi
  if ! have rustup; then
    info "rustup unavailable; skipped"
    return 0
  fi

  for target in $RUST_TARGETS; do
    if rustup target list --installed 2>/dev/null | grep -qx "$target"; then
      skip "target ${target}"
    elif [ "$DRY_RUN" -eq 1 ]; then
      info "would run: rustup target add ${target}"
    else
      rustup target add "$target"
      record "rustup-target:$target"
      ok "target ${target}"
    fi
  done

  if have cargo-ndk; then
    skip "cargo-ndk already installed"
  elif [ "$DRY_RUN" -eq 1 ]; then
    info "would run: cargo install cargo-ndk"
  else
    cargo install cargo-ndk
    record "cargo-bin:cargo-ndk"
    ok "cargo-ndk installed"
  fi
}

# --- shell profile -----------------------------------------------------------

detect_profile() {
  case "${SHELL:-}" in
    */zsh)  printf '%s' "$HOME/.zshrc" ;;
    */bash) printf '%s' "$HOME/.bash_profile" ;;
    *)      printf '%s' "$HOME/.profile" ;;
  esac
}

strip_profile_block() {
  # Remove a previously written marked block, keeping everything else.
  tmp="$(mktemp)"
  awk -v b="$PROFILE_BEGIN" -v e="$PROFILE_END" '
    $0 == b { inblock = 1 }
    inblock != 1 { print }
    $0 == e { inblock = 0 }
  ' "$1" >"$tmp"
  cat "$tmp" >"$1"
  rm -f "$tmp"
}

write_profile() {
  step "Shell profile"
  PROFILE_FILE="$(detect_profile)"
  ok "profile: ${PROFILE_FILE}"

  if [ "$DRY_RUN" -eq 1 ]; then
    info "would write the ANDROID_HOME / NDK_HOME / JAVA_HOME block"
    return 0
  fi

  touch "$PROFILE_FILE"
  strip_profile_block "$PROFILE_FILE"

  {
    printf '%s\n' "$PROFILE_BEGIN"
    printf '# Managed by setup-android-emulator.sh; safe to delete this block.\n'
    printf 'export JAVA_HOME="%s"\n' "$JAVA_HOME_DIR"
    printf 'export ANDROID_HOME="%s"\n' "$SDK_ROOT"
    printf 'export ANDROID_SDK_ROOT="%s"\n' "$SDK_ROOT"
    printf 'export NDK_HOME="%s"\n' "$NDK_HOME_DIR"
    # The literal $PATH must be written into the profile file, not expanded here.
    # shellcheck disable=SC2016
    printf 'export PATH="%s/cmdline-tools/latest/bin:%s/platform-tools:%s/emulator:$PATH"\n' \
      "$SDK_ROOT" "$SDK_ROOT" "$SDK_ROOT"
    printf '%s\n' "$PROFILE_END"
  } >>"$PROFILE_FILE"

  record "profile:${PROFILE_FILE}"
  ok "environment block written (idempotent; re-running replaces it)"
}

# --- emulator ----------------------------------------------------------------

# Print the serial of a running emulator that is using the given AVD, or nothing.
emulator_serial_for_avd() {
  local want="$1" s name
  local serials=()
  while IFS= read -r s; do
    if [ -n "$s" ]; then serials+=("$s"); fi
  done < <("$ADB" devices | awk '/^emulator-[0-9]+[[:space:]]+device$/ {print $1}')

  if [ "${#serials[@]}" -gt 0 ]; then
    for s in "${serials[@]}"; do
      name="$("$ADB" -s "$s" emu avd name 2>/dev/null | head -1 | tr -d '\r')"
      if [ "$name" = "$want" ]; then
        printf '%s' "$s"
        return 0
      fi
    done
  fi
  printf '%s' ""
}

wait_for_boot() {
  waited=0
  info "waiting for sys.boot_completed (timeout ${BOOT_TIMEOUT}s)"
  while [ "$waited" -lt "$BOOT_TIMEOUT" ]; do
    TARGET_SERIAL="$(emulator_serial_for_avd "$AVD_NAME")"
    if [ -n "$TARGET_SERIAL" ] \
      && [ "$("$ADB" -s "$TARGET_SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then
      ok "booted after ${waited}s on ${TARGET_SERIAL}"
      return 0
    fi
    sleep 5
    waited=$((waited + 5))
  done
  fail "AVD '${AVD_NAME}' did not finish booting within ${BOOT_TIMEOUT}s (see ${STATE_DIR}/emulator.log)."
}

start_emulator() {
  step "Starting the emulator"
  if [ "$NO_START" -eq 1 ]; then
    skip "--no-start given"
    return 0
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    info "would run: emulator -avd ${AVD_NAME} -no-snapshot-load"
    return 0
  fi
  [ -x "$ADB" ] || fail "adb not found at ${ADB}."
  [ -x "${SDK_ROOT}/emulator/emulator" ] || fail "emulator not found at ${SDK_ROOT}/emulator/emulator."

  if [ -n "$(emulator_serial_for_avd "$AVD_NAME")" ]; then
    ok "AVD '${AVD_NAME}' is already running"
  else
    mkdir -p "$STATE_DIR"
    nohup "${SDK_ROOT}/emulator/emulator" -avd "$AVD_NAME" \
      -no-snapshot-load -no-boot-anim >"${STATE_DIR}/emulator.log" 2>&1 &
    ok "emulator launched (log: ${STATE_DIR}/emulator.log)"

    # Catch an immediate failure (bad SDK root, missing image) instead of
    # waiting out the whole boot timeout for it.
    sleep 5
    if grep -q "^FATAL" "${STATE_DIR}/emulator.log" 2>/dev/null; then
      tail -20 "${STATE_DIR}/emulator.log" | sed 's/^/       /' >&2
      fail "the emulator exited during startup (see ${STATE_DIR}/emulator.log)."
    fi
  fi

  wait_for_boot
}

report_device() {
  step "Device"
  "$ADB" devices
  if [ -z "$TARGET_SERIAL" ]; then
    return 0
  fi
  release="$("$ADB" -s "$TARGET_SERIAL" shell getprop ro.build.version.release 2>/dev/null | tr -d '\r' || true)"
  api="$("$ADB" -s "$TARGET_SERIAL" shell getprop ro.build.version.sdk 2>/dev/null | tr -d '\r' || true)"
  model="$("$ADB" -s "$TARGET_SERIAL" shell getprop ro.product.model 2>/dev/null | tr -d '\r' || true)"
  ok "${TARGET_SERIAL}: Android ${release:-?} (API ${api:-?}) on ${model:-?}"
}

# --- summary -----------------------------------------------------------------

summary() {
  step "Summary"
  printf '  SDK          %s\n' "$SDK_ROOT"
  printf '  platform     %s\n' "${PLATFORM_PKG:-n/a}"
  printf '  build-tools  %s\n' "${BUILD_TOOLS_PKG:-n/a}"
  printf '  NDK          %s\n' "${NDK_PKG:-n/a}"
  printf '  system image %s\n' "${IMAGE_PKG:-n/a}"
  printf '  AVD          %s\n' "$AVD_NAME"
  printf '  profile      %s\n' "${PROFILE_FILE:-n/a}"
  printf '\n  Open a new shell (or: source %s) to pick up the environment.\n' "${PROFILE_FILE:-your profile}"
  printf '  Start:  emulator -avd %s\n' "$AVD_NAME"
  printf '  Stop:   adb emu kill\n'
}

# --- uninstall ---------------------------------------------------------------

uninstall() {
  step "Uninstall"
  if [ ! -f "$STATE_FILE" ]; then
    info "no manifest at ${STATE_FILE}; nothing was recorded as installed by this script."
    return 0
  fi

  printf 'This will remove only what this script installed:\n\n'
  sed 's/^/  /' "$STATE_FILE"
  printf '\n'
  reply="n"
  read -r -p "Proceed? [y/N] " reply || reply="n"
  case "$reply" in
    y|Y|yes|YES) ;;
    *) fail "aborted at your request." ;;
  esac

  while IFS= read -r line; do
    case "$line" in
      brew:*)            run brew uninstall "${line#brew:}" || true ;;
      rustup-target:*)   run rustup target remove "${line#rustup-target:}" || true ;;
      cargo-bin:*)       run cargo uninstall "${line#cargo-bin:}" || true ;;
      avd:*)             run "$AVDMANAGER" delete avd -n "${line#avd:}" || true ;;
      profile:*)         if [ "$DRY_RUN" -eq 1 ]; then
                           info "would remove the marked block from ${line#profile:}"
                         else
                           strip_profile_block "${line#profile:}"
                         fi ;;
      sdk-root-created:*) run rm -rf "${line#sdk-root-created:}" ;;
      sdk-pkg:*)         info "leaving SDK package (shared): ${line#sdk-pkg:}" ;;
      sdk:*)             info "SDK root: ${line#sdk:}" ;;
      *)                 info "ignoring unknown entry: ${line}" ;;
    esac
  done <"$STATE_FILE"

  run rm -f "$STATE_FILE"
  if [ "$DRY_RUN" -eq 1 ]; then
    ok "dry run: nothing was removed"
  else
    ok "uninstall complete"
  fi
}

# --- main --------------------------------------------------------------------

main() {
  parse_args "$@"

  SDKMANAGER="${SDK_ROOT}/cmdline-tools/latest/bin/sdkmanager"
  AVDMANAGER="${SDK_ROOT}/cmdline-tools/latest/bin/avdmanager"
  ADB="${SDK_ROOT}/platform-tools/adb"

  # The emulator, sdkmanager and Gradle all resolve the SDK from the
  # environment, so point them at *this* SDK rather than whatever the caller
  # happens to have exported (a stale ANDROID_HOME breaks AVD startup).
  export ANDROID_HOME="$SDK_ROOT"
  export ANDROID_SDK_ROOT="$SDK_ROOT"
  export PATH="${SDK_ROOT}/cmdline-tools/latest/bin:${SDK_ROOT}/platform-tools:${SDK_ROOT}/emulator:${HOME}/.cargo/bin:${PATH}"

  if [ "$UNINSTALL" -eq 1 ]; then
    uninstall
    return 0
  fi

  preflight
  ensure_brew
  ensure_java
  if [ "$SKIP_RUST" -eq 0 ]; then
    ensure_rustup
  fi
  ensure_cmdline_tools
  accept_licenses
  resolve_packages
  install_packages
  ensure_avd
  setup_rust
  write_profile
  start_emulator

  if [ "$NO_START" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
    report_device
  fi
  summary
}

main "$@"
