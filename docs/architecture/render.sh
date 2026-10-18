#!/usr/bin/env bash
# Render the architecture diagrams with D2.
#
# The d2 toolchain is kept project-local in .tools/ (gitignored) so nothing is
# installed globally. Run this once to fetch it, then again whenever a .d2
# source changes.
#
#   docs/architecture/render.sh            # render every *.d2 -> *.svg
#   docs/architecture/render.sh --png      # also emit *.png
#
# SVG is what is committed; PNG is handy for pasting into chat or slides.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"

D2_VERSION="v0.9.0"
D2_HOME="$root/.tools/d2"
D2="$D2_HOME/bin/d2"
THEME="${D2_THEME:-3}" # Flagship Terrastruct

# A download that is interrupted must not leave its temp dir behind.
tmp=""
cleanup() {
  [ -n "$tmp" ] || return 0
  rm -rf "$tmp"
}
trap cleanup EXIT

# Diagrams that get a second, animated rendering (--animate-interval reveals
# the connections in order, which reads as the event propagating).
ANIMATED=(04-sync 06-gossip)
ANIM_MS=900

case "$(uname -s)-$(uname -m)" in
Darwin-arm64) asset="d2-$D2_VERSION-macos-arm64.tar.gz" ;;
Darwin-x86_64) asset="d2-$D2_VERSION-macos-amd64.tar.gz" ;;
Linux-x86_64) asset="d2-$D2_VERSION-linux-amd64.tar.gz" ;;
Linux-aarch64 | Linux-arm64) asset="d2-$D2_VERSION-linux-arm64.tar.gz" ;;
*)
  echo "Unsupported platform: $(uname -s) $(uname -m)" >&2
  exit 1
  ;;
esac

install_d2() {
  echo "Installing d2 $D2_VERSION into $D2_HOME …"
  tmp="$(mktemp -d)"
  curl -fsSL -o "$tmp/d2.tar.gz" \
    "https://github.com/terrastruct/d2/releases/download/$D2_VERSION/$asset"
  tar -xzf "$tmp/d2.tar.gz" -C "$tmp"
  mkdir -p "$D2_HOME"
  cp -R "$tmp/d2-$D2_VERSION"/. "$D2_HOME/"
  rm -rf "$tmp"
  tmp=""
}

[ -x "$D2" ] || install_d2

want_png=false
[ "${1:-}" = "--png" ] && want_png=true

shopt -s nullglob
count=0
for src in "$here"/*.d2; do
  name="$(basename "$src" .d2)"
  "$D2" "$src" "$here/$name.svg" --theme "$THEME" --pad 24 --layout elk
  count=$((count + 1))

  if $want_png; then
    "$D2" "$src" "$here/$name.png" --theme "$THEME" --pad 24 --layout elk
  fi

  for anim in "${ANIMATED[@]}"; do
    if [ "$name" = "$anim" ]; then
      "$D2" "$src" "$here/$name-animated.svg" --theme "$THEME" --pad 24 \
        --animate-interval "$ANIM_MS"
    fi
  done
done

echo "Rendered $count diagram(s) from $here"
