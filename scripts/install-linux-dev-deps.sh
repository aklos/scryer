#!/usr/bin/env bash
# Install system libraries required to build the Scryer Tauri desktop app on
# Debian/Ubuntu. Safe to re-run — apt skips packages already installed.
#
# Run: bash scripts/install-linux-dev-deps.sh
set -euo pipefail

PKGS=(
  libwebkit2gtk-4.1-dev
  build-essential
  curl
  wget
  file
  libxdo-dev
  libssl-dev
  libayatana-appindicator3-dev
  librsvg2-dev
  patchelf
  pkg-config
)

if ! command -v apt-get >/dev/null 2>&1; then
  echo "This script targets Debian/Ubuntu (apt). For other distros, see:"
  echo "  https://v2.tauri.app/start/prerequisites/"
  exit 1
fi

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
  echo "Re-running with sudo…"
  exec sudo bash "$0" "$@"
fi

# apt update often fails on fresh Debian installs when the install CD is still
# listed in sources.list but not mounted. Install does not need a fresh update.
if ! apt-get update; then
  echo ""
  echo "warning: apt-get update failed (often a stale cdrom: entry in /etc/apt/sources.list)."
  echo "         Continuing with install anyway…"
  echo "         To fix later: sudo sed -i 's|^deb cdrom|# deb cdrom|' /etc/apt/sources.list"
  echo ""
fi

apt-get install -y "${PKGS[@]}"

if pkg-config --exists glib-2.0; then
  echo ""
  echo "OK — glib-2.0 found. Try: pnpm tauri dev"
else
  echo ""
  echo "error: packages installed but pkg-config still cannot find glib-2.0."
  echo "       Check: pkg-config --modversion glib-2.0"
  exit 1
fi
