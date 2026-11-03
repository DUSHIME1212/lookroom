#!/usr/bin/env bash
# Installs Lookroom for the current user: the program in ~/.local/bin, plus its icon and
# menu entry. Nothing outside your home folder is touched.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bin="${HOME}/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps"

mkdir -p "$bin" "$apps" "$icons"
install -m 755 "$here/lookroom" "$bin/lookroom"
install -m 644 "$here/lookroom.png" "$icons/lookroom.png"
sed "s|^Exec=.*|Exec=${bin}/lookroom %F|" "$here/lookroom.desktop" > "$apps/lookroom.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$apps" || true
echo "Lookroom is installed. Start it from your applications menu, or run ${bin}/lookroom."
command -v ffmpeg >/dev/null || echo "Video needs ffmpeg: sudo apt install ffmpeg (or your distribution's package)."
