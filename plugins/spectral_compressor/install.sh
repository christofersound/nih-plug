#!/usr/bin/env bash
# Installs Sound Digger (by NoTempo) into a "NoTempo" folder for VST3 and CLAP on macOS.
# Usage: ./install.sh [--system] [path/to/target/bundled]
set -euo pipefail

NAME="Sound Digger (by NoTempo)"
BASE="$HOME/Library/Audio/Plug-Ins"
SUDO=""
BUNDLED=""

for arg in "$@"; do
  case "$arg" in
    --system) BASE="/Library/Audio/Plug-Ins"; SUDO="sudo" ;;
    *) BUNDLED="$arg" ;;
  esac
done

# By default look for the bundles in the nih-plug workspace this folder lives in
if [ -z "$BUNDLED" ]; then
  HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  BUNDLED="$HERE/../../target/bundled"
fi

install_bundle() {
  local ext="$1" format_dir="$2"
  local src="$BUNDLED/$NAME.$ext"
  local dest="$BASE/$format_dir/NoTempo"

  if [ ! -d "$src" ] && [ ! -f "$src" ]; then
    echo "Skipping $ext: '$src' not found. Run: cargo xtask bundle sound_digger --release"
    return
  fi

  $SUDO mkdir -p "$dest"
  $SUDO rm -rf "$dest/$NAME.$ext"
  $SUDO cp -R "$src" "$dest/"
  # Remove the quarantine flag and ad-hoc sign the bundle, needed for unsigned builds on Apple Silicon
  $SUDO xattr -cr "$dest/$NAME.$ext" || true
  $SUDO codesign --force --deep --sign - "$dest/$NAME.$ext" || true
  echo "Installed $dest/$NAME.$ext"
}

install_bundle vst3 VST3
install_bundle clap CLAP
