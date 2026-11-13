# Lookroom for iPhone — xtool

Status: native mobile app compiled and packaged with xtool; **not signed for a device or installed**. Requires iOS 17 or later. The app display name is Lookroom and its icon is an exact copy of `app/assets/icon.png`.

## Current setup

- The Lookroom workflow uses xtool 1.24.0 from `~/.local/share/lookroom-toolchains/xtool-1.24.0`. The older `/usr/local/bin/xtool` is preserved. Source `ios/env.sh` to select the updated version. Version 1.24.0 includes Apple authentication changes released after 1.22.2; this is not a guarantee that Apple will accept a login.
- Swift 6.4.2 is installed under `~/.local/share/lookroom-toolchains/swift-6.4.2`.
- `env.sh` makes that toolchain available without modifying your shell configuration. Set `LOOKROOM_SWIFT_DIR` if using another installation.
- The Darwin Swift SDK is installed from `Xcode_27.2.xip`, including iPhoneOS 27.2.
- An iPhone is visible on USB, but `xtool devices` was still waiting for a usable connection.

## Finish setup on this computer

1. The SDK is already installed on this computer. On another computer, download the required Xcode archive from https://developer.apple.com/download/all/ and install it through xtool. Apple requires you to sign in and accept its download terms directly.
2. In your own terminal, run:

```bash
cd /home/donaime/Desktop/lookroom
source ios/env.sh
xtool setup
```

Complete Apple's sign-in and two-factor prompts privately. Give xtool the local Xcode.xip path when asked. Official setup: https://xtool.sh/documentation/xtool/installation-linux/.

3. Connect and unlock the iPhone. Accept the trust/pairing prompt and enable Developer Mode if required by the phone. Confirm `xtool devices` recognizes it.
4. Build, sign, install and launch:

```bash
./ios/run-iphone.sh
```

The bundle ID is `com.donaime.lookroom`; change it in `LookroomMobile/xtool.yml` if your Apple team requires another identifier.

## Mobile scope

This is a native SwiftUI/Core Image companion, not the desktop Rust/egui executable repackaged. It includes local recent projects, import from Files, photo preview, native video playback, exposure/contrast/saturation/warmth/sharpen/vignette adjustments, JPEG export, video export with original audio through AVFoundation, and export sharing. Video export is cancellable. Preview images are reduced before grading; video uses the native composition pipeline rather than collecting all decoded frames.

Desktop `.lookroom` projects, timeline/multilayer editing, templates, LUTs, masks, advanced grading, and the Studium XR plugin are not ported. Mobile projects use their own JSON format and copied media in the app's Documents/Lookroom directory.

## Verification

Swift source syntax parsing, SwiftPM manifest validation, shell syntax, plist parsing, and byte-for-byte icon comparison passed. A small compiler check also verified the dispatch/main-actor pattern used by the preview model in Swift 5 language mode. The app now compiles against iPhoneOS 27.2 without reported errors or warnings, and `xtool dev build --ipa` produced `LookroomMobile/xtool/LookroomMobile.ipa` (about 3.8 MiB). Its Info.plist confirms the display name Lookroom, bundle ID `com.donaime.lookroom` and minimum iOS 17.0; the packaged icon is byte-for-byte identical to the desktop icon. The native AVPlayerViewController is bridged into SwiftUI because this SDK did not expose the VideoPlayer overlay. Device signing, installation and runtime tests remain pending.
