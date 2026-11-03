#!/usr/bin/env bash
# Builds Lookroom for Linux and packs it with its icon, desktop entry and installer:
# dist/lookroom-linux-<arch>.tar.gz. Run from the app folder on a Linux machine.
#
# Needs: Rust, pkg-config and the development packages for ALSA, GTK 3 and xkbcommon
# (Debian and Ubuntu: sudo apt install pkg-config libasound2-dev libgtk-3-dev libxkbcommon-dev).
# Lookroom uses ffmpeg at run time for video: sudo apt install ffmpeg.
set -euo pipefail
cd "$(dirname "$0")/.."

arch="$(uname -m)"
name="lookroom-linux-${arch}"
out="dist/${name}"

cargo build --release
rm -rf "$out" && mkdir -p "$out"
cp target/release/lookroom "$out/"
target/release/lookroom --icon "$out/lookroom.png" 512
cp packaging/linux/lookroom.desktop packaging/linux/install.sh MECHANICS.md "$out/"
chmod +x "$out/install.sh" "$out/lookroom"
tar -C dist -czf "dist/${name}.tar.gz" "$name"
echo "Made dist/${name}.tar.gz"
