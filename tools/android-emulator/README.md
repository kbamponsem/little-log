# Android emulator setup (macOS)

One script that installs and configures an Android emulator on macOS with no
manual steps, then prepares the Rust + NDK toolchain for a Tauri 2 Android app.

```bash
./setup-android-emulator.sh
```

It is idempotent — re-running it skips whatever is already in place.

## Usage

```
setup-android-emulator.sh [options]

  --dry-run         print what would happen, change nothing
  --avd-name NAME   AVD name to create (default: little-log-pixel)
  --skip-rust       skip the Rust targets + cargo-ndk step
  --no-start        do not start the emulator
  --uninstall       remove only what this script installed (asks first)
  -h, --help        show this help
```

Useful env overrides: `ANDROID_SDK_DIR` (SDK location) and `BOOT_TIMEOUT`
(seconds to wait for boot, default 300).

## What gets installed, and where

| Thing | Location |
|---|---|
| Android SDK | `$HOME/Library/Android/sdk` |
| cmdline-tools | `$SDK/cmdline-tools/latest` |
| platform-tools (`adb`), `emulator` | `$SDK/platform-tools`, `$SDK/emulator` |
| Platform, build-tools, NDK, system image | resolved at run time (see below) |
| AVD | `$HOME/.android/avd/<name>.avd` |
| JDK 17 | Homebrew `openjdk@17` |
| `cargo-ndk` | `$HOME/.cargo/bin` |
| Install manifest | `$HOME/.little-log-android-setup/manifest` |
| Emulator log | `$HOME/.little-log-android-setup/emulator.log` |
| Shell env block | `$HOME/.zshrc` (or `.bash_profile`), between marked comments |

No versions are hardcoded. The cmdline-tools build is scraped from
`developer.android.com/studio`, and the platform, build-tools, NDK and system
image are the latest *stable* entries in `sdkmanager --list` (anything marked
`rc`/`beta`/`alpha` is skipped). The NDK is required to be r28 or newer, for
16 KB page alignment. On Apple Silicon the system image is `arm64-v8a`; on Intel
it is `x86_64`.

If the newest platform has no Google Play image for your ABI, the script falls
back to the newest one that does — that is why you may see, for example,
`platforms;android-37.2` alongside a `android-36` Play image.

Everything the script installs is recorded in the manifest, which is what
`--uninstall` reads.

## Starting and stopping the emulator

```bash
emulator -avd little-log-pixel        # start (opens a window)
emulator -avd little-log-pixel -no-window -no-audio   # headless
adb devices                            # list running devices
adb emu kill                           # stop the running emulator
emulator -list-avds                    # list AVDs
```

`setup-android-emulator.sh` starts the AVD for you unless you pass `--no-start`.
Open a new shell (or `source ~/.zshrc`) first so `emulator` and `adb` resolve to
the new SDK.

## Uninstalling

```bash
./setup-android-emulator.sh --uninstall
```

It prints the manifest, asks for confirmation, and then removes only what it
installed: the Rust targets it added, `cargo-ndk`, the AVD, the marked profile
block, and the SDK directory *if the script created it*. SDK packages are left
in place, because they may be shared with other tooling. Nothing is removed
without confirmation; `--uninstall --dry-run` prints the plan and changes
nothing.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `HAXM is not installed` / `emulator: ERROR: x86 emulation currently requires hardware acceleration` | HAXM is Intel-only and deprecated. On Apple Silicon this message means you are running an `x86_64` image on an arm64 host — recreate the AVD from an `arm64-v8a` image (the script picks this by CPU automatically). On Intel, check `emulator -accel-check`; install HAXM only if it is still supported on your macOS version, otherwise use `-accel auto`. |
| `FATAL \| Cannot find AVD system path. Please define ANDROID_SDK_ROOT` | The emulator resolved a different SDK root from the environment. Make sure `ANDROID_SDK_ROOT`/`ANDROID_HOME` point at the SDK that actually contains the AVD's system image, or open a new shell so the script's profile block applies. |
| Emulator is very slow | Make sure `hw.gpu.mode=host` (the script sets this) and that you are not running on an `x86_64` image under emulation on Apple Silicon. Do not run two AVDs at once — each takes 4 GB. `-no-snapshot-load` trades a slower boot for a predictable one. |
| `adb devices` shows `offline` or nothing | `adb kill-server && adb start-server`, then `adb devices`. A device can sit `offline` for ~30 s after boot. If a device shows `unauthorized`, accept the USB-debugging prompt on the phone. |
| `Failed to install ... licenses not accepted` / licence prompts block a CI run | `yes \| sdkmanager --licenses` (the script already does this). If it still fails, run it manually with `--sdk_root="$ANDROID_HOME"`. |
| `Warning: The SDK Manager CLI tool (sdkmanager) is deprecated` | Informational. `sdkmanager` still works; Google now points at the newer `android` CLI in the same `cmdline-tools` directory. The script deliberately uses `cmdline-tools`, never the old `tools/` binaries. |
| `avdmanager` prints `Could not load devices from .../devices.xml` | Harmless. Google Play system images ship no `devices.xml`; the AVD is still created correctly. The script verifies the resulting `config.ini` instead of trusting the exit code. |
| An existing AVD suddenly fails with `Missing system image` | That AVD was created against a different SDK root. AVDs store a *relative* image path, so switching `ANDROID_HOME` can orphan them — either recreate the AVD from this SDK or point `ANDROID_HOME` back at the old SDK for that AVD. |
