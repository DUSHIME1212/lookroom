#!/usr/bin/env bash
set -euo pipefail
lookroom_ios_dir="$(cd "$(dirname "$0")" && pwd)"
source "$lookroom_ios_dir/env.sh"
cd "$lookroom_ios_dir/LookroomMobile"
exec xtool dev "$@"
