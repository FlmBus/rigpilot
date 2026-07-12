#!/usr/bin/env bash
#
# RigPilot macOS installer.
#
# Downloads the latest release .dmg for this Mac's architecture, installs
# RigPilot.app into /Applications, and strips the Gatekeeper quarantine flag
# so the un-notarized build launches without the "app is damaged" error.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/FlmBus/rigpilot/main/scripts/install-macos.sh | bash
#
# Env overrides:
#   RIGPILOT_REPO      GitHub repo (default: FlmBus/rigpilot)
#   RIGPILOT_APPDIR    install target (default: /Applications)

set -euo pipefail

REPO="${RIGPILOT_REPO:-FlmBus/rigpilot}"
APPDIR="${RIGPILOT_APPDIR:-/Applications}"
APP_NAME="RigPilot.app"

info()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m==>\033[0m %s\n' "$*" >&2; }
die()   { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "This installer is for macOS only."

# Map the CPU to the release asset's architecture suffix.
case "$(uname -m)" in
  arm64)  arch_suffix="aarch64" ;;
  x86_64) arch_suffix="x64" ;;
  *)      die "Unsupported architecture: $(uname -m)" ;;
esac

info "Looking up the latest RigPilot release for $arch_suffix..."
api_url="https://api.github.com/repos/${REPO}/releases/latest"

# Pick the .dmg whose name ends in _<arch>.dmg. No jq dependency: the GitHub
# API is line-oriented enough to grep the browser_download_url out directly.
dmg_url="$(
  curl -fsSL "$api_url" \
    | grep -oE '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]+"' \
    | sed -E 's/.*"(https[^"]+)"/\1/' \
    | grep -E "_${arch_suffix}\.dmg$" \
    | head -n1
)"

[ -n "$dmg_url" ] || die "No _${arch_suffix}.dmg asset found in the latest release."
info "Found: ${dmg_url##*/}"

# Everything lands in a temp dir that we always clean up.
workdir="$(mktemp -d)"
mnt="$workdir/mnt"
mkdir -p "$mnt"
cleanup() {
  hdiutil detach "$mnt" -quiet 2>/dev/null || true
  rm -rf "$workdir"
}
trap cleanup EXIT

dmg="$workdir/rigpilot.dmg"
info "Downloading..."
curl -fSL --progress-bar "$dmg_url" -o "$dmg"

info "Mounting the disk image..."
hdiutil attach "$dmg" -nobrowse -quiet -mountpoint "$mnt"

[ -d "$mnt/$APP_NAME" ] || die "$APP_NAME not found inside the disk image."

target="$APPDIR/$APP_NAME"
if [ -e "$target" ]; then
  info "Removing the previous install at $target..."
  rm -rf "$target"
fi

info "Installing to $target..."
cp -R "$mnt/$APP_NAME" "$APPDIR/"

info "Removing the Gatekeeper quarantine flag..."
xattr -dr com.apple.quarantine "$target" || \
  warn "Could not strip quarantine — you may need to run: xattr -dr com.apple.quarantine \"$target\""

info "Done. Launch RigPilot from your Applications folder or with: open \"$target\""
