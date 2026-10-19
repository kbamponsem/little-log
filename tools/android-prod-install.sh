#!/usr/bin/env bash
#
# Build the production (release) APK for one connected Android device, copy it
# into the app directory so it is easy to share, and install it on that device.
#
# Usage:
#   tools/android-prod-install.sh <device-id>
#   tools/android-prod-install.sh 2A271FDH2001E3
#
# The target ABI is read from the device, so only that one is compiled.
# Signing is configured by app/src-tauri/gen/android/keystore.properties.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_DIR="$ROOT_DIR/app"
GRADLE_DIR="$APP_DIR/src-tauri/gen/android"
BUILT_APK="$GRADLE_DIR/app/build/outputs/apk/universal/release/app-universal-release.apk"

# The version comes from the Tauri config, so shared files are distinguishable.
CONF="$APP_DIR/src-tauri/tauri.conf.json"
VERSION=""
if [ -f "$CONF" ]; then
  VERSION="$(awk -F'"' '/^[[:space:]]*"version"[[:space:]]*:/ { print $4; exit }' "$CONF")"
fi
VERSION="${VERSION:-dev}"

info() { printf '       %s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }
fail() { printf '[fail] %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<EOF
Usage: ${0##*/} <device-id>

Builds the production (release) APK for the given adb device, copies it to
${APP_DIR#"$ROOT_DIR"/}/, and installs it. The target ABI is read from the device.

Example:
  ${0##*/} 2A271FDH2001E3
EOF
}

case "${1:-}" in
  -h | --help) usage; exit 0 ;;
esac

if [ "$#" -ne 1 ]; then
  usage >&2
  exit 2
fi

DEVICE="$1"

# --- Android SDK / adb ------------------------------------------------------

# Tauri and Gradle both need the SDK in the environment; fall back to the
# usual install locations when the caller has not exported it.
if [ -z "${ANDROID_HOME:-}" ] && [ -z "${ANDROID_SDK_ROOT:-}" ]; then
  for sdk in "$HOME/Android" "$HOME/Library/Android/sdk"; do
    if [ -d "$sdk/platform-tools" ]; then
      export ANDROID_HOME="$sdk"
      export ANDROID_SDK_ROOT="$sdk"
      break
    fi
  done
fi
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"

ADB="$(command -v adb || true)"
if [ -z "$ADB" ] && [ -n "$SDK" ] && [ -x "$SDK/platform-tools/adb" ]; then
  ADB="$SDK/platform-tools/adb"
fi
[ -n "$ADB" ] || fail "adb not found. Install platform-tools or set ANDROID_HOME."

# --- Preflight --------------------------------------------------------------

state="$("$ADB" -s "$DEVICE" get-state 2>/dev/null || true)"
[ "$state" = "device" ] || fail "device '$DEVICE' is not connected (adb get-state: ${state:-offline})."

abi="$("$ADB" -s "$DEVICE" shell getprop ro.product.cpu.abi 2>/dev/null | tr -d '\r')"
case "$abi" in
  arm64-v8a)   target="aarch64" ;;
  armeabi-v7a) target="armv7" ;;
  x86_64)      target="x86_64" ;;
  x86)         target="i686" ;;
  *)           fail "unsupported device ABI: ${abi:-unknown}" ;;
esac

[ -f "$GRADLE_DIR/keystore.properties" ] ||
  fail "missing $GRADLE_DIR/keystore.properties; release signing is not configured."
[ -d "$APP_DIR/node_modules" ] ||
  fail "app/node_modules is missing; run 'npm install' in $APP_DIR first."

# The shareable copy: stable name, version and ABI in the file name.
SHARED_APK="$APP_DIR/little-log-${VERSION}-${abi}.apk"

model="$("$ADB" -s "$DEVICE" shell getprop ro.product.model 2>/dev/null | tr -d '\r' || true)"

# --- Build ------------------------------------------------------------------

step "Building Little Log release for ${model:-device} (${abi} -> ${target})"
(
  cd "$APP_DIR"
  npm run android:build -- --target "$target"
)

[ -f "$BUILT_APK" ] || fail "build finished but the APK was not found at $BUILT_APK"

# --- Publish the shareable copy --------------------------------------------

cp "$BUILT_APK" "$SHARED_APK"
info "apk: $SHARED_APK"

# --- Install ----------------------------------------------------------------

step "Installing on $DEVICE"
"$ADB" -s "$DEVICE" install -r "$SHARED_APK"

info "installed: $SHARED_APK"
info "copy this file to share it with someone else."
