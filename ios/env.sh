#!/usr/bin/env bash
# Source this file before xtool setup, or use run-iphone.sh.
lookroom_xtool_dir="${LOOKROOM_XTOOL_DIR:-$HOME/.local/share/lookroom-toolchains/xtool-1.24.0}"
if [ -x "$lookroom_xtool_dir/xtool" ]; then
    export PATH="$lookroom_xtool_dir:$PATH"
fi
lookroom_swift_dir="${LOOKROOM_SWIFT_DIR:-$HOME/.local/share/lookroom-toolchains/swift-6.4.2}"
if [ -x "$lookroom_swift_dir/usr/bin/swift" ]; then
    export PATH="$lookroom_swift_dir/usr/bin:$PATH"
fi
if ! command -v swift >/dev/null 2>&1; then
    echo 'Swift is missing. Install Swift or set LOOKROOM_SWIFT_DIR to its toolchain directory.' >&2
    return 1 2>/dev/null || exit 1
fi
