#!/usr/bin/env bash
# Builds shell2 for the iOS simulator, wraps it in a .app, installs and launches it on the first
# available iPhone simulator, then saves a screenshot, the app's log and any crash report to
# ios/build/sim/. macOS only; run from the repo root (docs/design/ios.md I1–I2).
# Env passed through to the app: OSVAULD_SOCKET, OSVAULD_DATA_DIR (as SIMCTL_CHILD_*).
set -euo pipefail
T=aarch64-apple-ios-sim; ID=com.osvauld.shell; OUT=ios/build/sim; APP=ios/build/Osvauld.app
# Same minimum for rustc and every cc build (luau, ring, ...); keep in step with Info.plist.
export IPHONEOS_DEPLOYMENT_TARGET=17.0
rm -rf "$OUT" "$APP"; mkdir -p "$OUT" "$APP"
rustup target add "$T"
cargo build -j 6 -p shell2 --no-default-features --target "$T"
# iOS 26's SDK refuses to launch an app without the UIScene lifecycle, which winit 0.30 lacks
# (winit PR #4723). Stamping the binary as built against SDK 18 keeps the old lifecycle.
# Simulator-only stopgap: an App Store build needs real UIScene support (docs/design/ios.md §1).
xcrun vtool -set-build-version 7 17.0 18.0 -replace -output "$APP/shell2" "target/$T/debug/shell2"
cp scripts/ios/Info.plist "$APP/Info.plist"
# Bundled apps: installed into a new account's "Apps" space at signup (shell2/src/bundled.rs).
mkdir -p "$APP/apps" && cp -R demo_apps/jetpack "$APP/apps/jetpack"
codesign --force --sign - "$APP"
UDID=$(xcrun simctl list devices available | grep -m1 -E '^ +iPhone' | grep -oE '[0-9A-F-]{36}')
echo "simulator: $(xcrun simctl list devices available | grep "$UDID")"
xcrun simctl boot "$UDID" 2>/dev/null || true
xcrun simctl bootstatus "$UDID" -b >/dev/null
xcrun simctl terminate "$UDID" "$ID" 2>/dev/null || true
xcrun simctl install "$UDID" "$APP"
[ -n "${OSVAULD_SOCKET:-}" ] && export SIMCTL_CHILD_OSVAULD_SOCKET="$OSVAULD_SOCKET"
[ -n "${OSVAULD_DATA_DIR:-}" ] && export SIMCTL_CHILD_OSVAULD_DATA_DIR="$OSVAULD_DATA_DIR"
export SIMCTL_CHILD_RUST_BACKTRACE=1 SIMCTL_CHILD_RUST_LOG="${RUST_LOG:-info}"
start=$(date '+%Y-%m-%d %H:%M:%S')
xcrun simctl launch --stdout="$PWD/$OUT/stdout.log" --stderr="$PWD/$OUT/stderr.log" "$UDID" "$ID"
sleep 10
xcrun simctl io "$UDID" screenshot "$OUT/launch.png"
# grep -c, not -q: -q closes the pipe early and pipefail reports launchctl's SIGPIPE.
if [ "$(xcrun simctl spawn "$UDID" launchctl list | grep -c "$ID")" -gt 0 ]; then echo "app: running"; else echo "app: NOT running"; fi
xcrun simctl spawn "$UDID" log show --start "$start" --style compact --predicate 'process == "shell2"' > "$OUT/app.log" 2>&1 || true
find ~/Library/Logs/DiagnosticReports -name 'shell2*' -newermt "$start" -exec cp {} "$OUT/" \; 2>/dev/null || true
ls "$OUT"
