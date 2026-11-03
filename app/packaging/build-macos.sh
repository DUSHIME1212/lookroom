#!/usr/bin/env bash
# Builds Lookroom for macOS as one app for both Apple Silicon and Intel Macs:
# dist/Lookroom.app and dist/lookroom-macos-universal.zip. Run from the app folder on a Mac.
#
# Needs: Rust with both Mac targets (rustup target add aarch64-apple-darwin x86_64-apple-darwin)
# and the Xcode command line tools (xcode-select --install).
# Lookroom uses ffmpeg at run time for video: brew install ffmpeg.
set -euo pipefail
cd "$(dirname "$0")/.."

targets=(aarch64-apple-darwin x86_64-apple-darwin)
for t in "${targets[@]}"; do
  cargo build --release --target "$t"
done

app="dist/Lookroom.app"
rm -rf "$app" && mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/lookroom" \
  "target/aarch64-apple-darwin/release/lookroom" "target/x86_64-apple-darwin/release/lookroom"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
sed "s/@VERSION@/${version}/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"

# The icon, drawn by the app itself at every size macOS asks for.
set_dir="dist/lookroom.iconset"
rm -rf "$set_dir" && mkdir -p "$set_dir"
for s in 16 32 128 256 512; do
  "$app/Contents/MacOS/lookroom" --icon "$set_dir/icon_${s}x${s}.png" "$s"
  "$app/Contents/MacOS/lookroom" --icon "$set_dir/icon_${s}x${s}@2x.png" "$((s * 2))"
done
iconutil -c icns -o "$app/Contents/Resources/lookroom.icns" "$set_dir"
rm -rf "$set_dir"

# Signed for this Mac only. Others open it the first time with right-click, Open,
# until it is signed with a Developer ID and notarised.
codesign --force --deep --sign - "$app"
ditto -c -k --keepParent "$app" dist/lookroom-macos-universal.zip
echo "Made $app and dist/lookroom-macos-universal.zip"
