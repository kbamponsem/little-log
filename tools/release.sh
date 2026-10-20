#!/usr/bin/env bash
#
# Bump the app version, commit it, tag it, and push the tag.
#
# Pushing the tag triggers .github/workflows/android-release.yml, which builds
# the Android APK and publishes the GitHub release. Only the version in
# app/src-tauri/tauri.conf.json matters: Tauri derives the APK versionName and
# versionCode from it.
#
# Usage:
#   tools/release.sh                 # 0.1.1 -> 0.1.2
#   tools/release.sh minor           # 0.1.1 -> 0.2.0
#   tools/release.sh major           # 0.1.1 -> 1.0.0
#   tools/release.sh 0.2.3           # explicit
#   tools/release.sh --dry-run minor # show the next version, change nothing
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CONF="$ROOT_DIR/app/src-tauri/tauri.conf.json"
BRANCH="main"

info() { printf '       %s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }
fail() { printf '[fail] %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<EOF
Usage: ${0##*/} [--dry-run] [patch|minor|major|X.Y.Z]

Bumps app/src-tauri/tauri.conf.json, commits it, tags it as v<version>, and
pushes main and the tag. The tag starts the Android release workflow.

Examples:
  ${0##*/}             # patch bump
  ${0##*/} minor
  ${0##*/} 0.2.0
  ${0##*/} --dry-run minor
EOF
}

# --- Arguments --------------------------------------------------------------

DRY_RUN=0
ARGS=()
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    -h | --help) usage; exit 0 ;;
    *) ARGS+=("$arg") ;;
  esac
done

[ "${#ARGS[@]}" -le 1 ] || fail "too many arguments (try --help)"
bump="${ARGS[0]:-patch}"

# --- Preflight --------------------------------------------------------------

[ -f "$CONF" ] || fail "missing $CONF"
cd "$ROOT_DIR"

[ "$(git branch --show-current)" = "$BRANCH" ] ||
  fail "must be on '$BRANCH' (currently '$(git branch --show-current)')."

current="$(awk -F'"' '/^[[:space:]]*"version"[[:space:]]*:/ { print $4; exit }' "$CONF")"
[[ "$current" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
  fail "cannot read a semver from $CONF (found '$current')."

IFS=. read -r major minor patch <<< "$current"
major=$((10#$major))
minor=$((10#$minor))
patch=$((10#$patch))

case "$bump" in
  patch) next="$major.$minor.$((patch + 1))" ;;
  minor) next="$major.$((minor + 1)).0" ;;
  major) next="$((major + 1)).0.0" ;;
  *)
    next="$bump"
    [[ "$next" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
      fail "not 'patch', 'minor', 'major' or a version like 0.2.0: $bump"
    ;;
esac

[ "$next" != "$current" ] || fail "version is already $current."

if [ "$DRY_RUN" -eq 1 ]; then
  info "would release v$next (current $current); nothing changed"
  exit 0
fi

[ -z "$(git status --porcelain)" ] ||
  fail "working tree is not clean; commit or stash your changes first."

git fetch origin "$BRANCH" --tags --quiet

[ "$(git rev-parse HEAD)" = "$(git rev-parse "origin/$BRANCH")" ] ||
  fail "local $BRANCH is not up to date with origin/$BRANCH; pull first."

[ -z "$(git rev-parse -q --verify "refs/tags/v$next" || true)" ] ||
  fail "tag v$next already exists locally."
[ -z "$(git ls-remote --tags origin "refs/tags/v$next")" ] ||
  fail "tag v$next already exists on origin."

# --- Edit, commit, tag, push ------------------------------------------------

step "Version $current -> $next"

awk -v v="$next" '
  !done && /^[[:space:]]*"version"[[:space:]]*:/ {
    sub(/"version"[[:space:]]*:[[:space:]]*"[^"]*"/, "\"version\": \"" v "\"")
    done = 1
  }
  { print }
' "$CONF" > "$CONF.tmp"
mv "$CONF.tmp" "$CONF"

grep -q "\"version\": \"$next\"" "$CONF" ||
  fail "could not update the version in $CONF."

git add "$CONF"
git commit -m "Release v$next" --quiet
git tag -a "v$next" -m "Little Log v$next"
git push origin "$BRANCH" "v$next"

remote="$(git remote get-url origin)"
slug="${remote#*github.com[:/]}"
slug="${slug%.git}"

info "pushed main and v$next"
info "release:  https://github.com/$slug/releases/tag/v$next"
info "workflow: https://github.com/$slug/actions/workflows/android-release.yml"
