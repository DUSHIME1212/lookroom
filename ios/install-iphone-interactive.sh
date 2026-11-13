#!/usr/bin/env bash
# Authentication stays in this desktop terminal. No credentials are logged.
set -euo pipefail
lookroom_ios_dir="$(cd "$(dirname "$0")" && pwd)"
lookroom_status_file=/tmp/lookroom-iphone-install.status
phase() { printf '%s\n' "$1" > "$lookroom_status_file"; }
failed() {
    phase failed
    printf '\nSetup stopped. The error is shown above in this terminal.\n'
    read -r -p 'Press Enter to close this window…' || true
}
trap failed ERR
phase preparing
source "$lookroom_ios_dir/env.sh"
printf '\nLookroom iPhone installation\n'
printf 'Enter your Apple sign-in details here, directly with xtool.\n'
printf 'For a normal Apple ID, select login mode 1 when prompted.\n\n'
xtool --version
phase awaiting_apple_sign_in
xtool auth login
phase awaiting_iphone
printf '\nConnect and unlock your iPhone, and accept Trust This Computer if prompted.\n'
xtool devices --usb
phase installing
"$lookroom_ios_dir/run-iphone.sh" run --usb
phase completed
printf '\nxtool finished installing Lookroom. Check your iPhone.\n'
read -r -p 'Press Enter to close this window…' || true
