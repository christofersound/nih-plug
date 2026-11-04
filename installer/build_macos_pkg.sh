#!/usr/bin/env bash
# Run from the repo root after bundling. Installs into .../NoTempo folders.
set -euo pipefail
B="target/bundled"
S="$(mktemp -d)"
mkdir -p "$S/Library/Audio/Plug-Ins/VST3/NoTempo" "$S/Library/Audio/Plug-Ins/CLAP/NoTempo"
cp -R "$B/Ezpectral Comp.vst3" "$S/Library/Audio/Plug-Ins/VST3/NoTempo/"
cp -R "$B/Ezpectral Comp.clap" "$S/Library/Audio/Plug-Ins/CLAP/NoTempo/"
pkgbuild --root "$S" --identifier com.notempo.ezpectral-comp --version 1.0.0 \
  --install-location "/" target/Ezpectral-Comp-Installer.pkg
