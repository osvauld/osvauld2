#!/usr/bin/env bash
# Builds shell2 for a real iPhone, bundles Jetpack, signs with the developer's own Apple account and
# installs + launches it on the attached phone. macOS only; run from the repo root. Untested as
# written (docs/design/ios.md §2 step 4): expect first-run fixes.
#
# Needs, once: Xcode signed into the Apple account; a provisioning profile for BUNDLE_ID (make a
# throwaway iOS app in Xcode with that bundle id, pick your team, run it on the phone once; Xcode
# saves the profile); the phone in Developer Mode and trusted.
#   OSVAULD_IOS_BUNDLE_ID  bundle id the profile was made for, e.g. com.yourname.osvauld
#   OSVAULD_IOS_PROFILE    path to that .mobileprovision
#   OSVAULD_IOS_IDENTITY   codesign identity, e.g. "Apple Development: you@example.com (TEAMID)"
#                          (`security find-identity -v -p codesigning`)
#   OSVAULD_IOS_DEVICE     device id from `xcrun devicectl list devices`
set -euo pipefail
: "${OSVAULD_IOS_BUNDLE_ID:?}" "${OSVAULD_IOS_PROFILE:?}" "${OSVAULD_IOS_IDENTITY:?}" "${OSVAULD_IOS_DEVICE:?}"
T=aarch64-apple-ios; OUT=ios/build/device; APP=$OUT/Osvauld.app
export IPHONEOS_DEPLOYMENT_TARGET=17.0
rm -rf "$OUT"; mkdir -p "$APP"
rustup target add "$T"
cargo build -p shell2 --no-default-features --target "$T"
# UIScene stopgap, as in run_sim.sh: platform 2 = iOS device.
xcrun vtool -set-build-version 2 17.0 18.0 -replace -output "$APP/shell2" "target/$T/debug/shell2"
cp scripts/ios/Info.plist "$APP/Info.plist"
plutil -replace CFBundleIdentifier -string "$OSVAULD_IOS_BUNDLE_ID" "$APP/Info.plist"
plutil -replace CFBundleSupportedPlatforms -json '["iPhoneOS"]' "$APP/Info.plist"
mkdir -p "$APP/apps" && cp -R demo_apps/jetpack "$APP/apps/jetpack"
cp "$OSVAULD_IOS_PROFILE" "$APP/embedded.mobileprovision"
security cms -D -i "$OSVAULD_IOS_PROFILE" > "$OUT/profile.plist"
plutil -extract Entitlements xml1 -o "$OUT/entitlements.plist" "$OUT/profile.plist"
codesign --force --sign "$OSVAULD_IOS_IDENTITY" --entitlements "$OUT/entitlements.plist" "$APP"
xcrun devicectl device install app --device "$OSVAULD_IOS_DEVICE" "$APP"
xcrun devicectl device process launch --console --device "$OSVAULD_IOS_DEVICE" "$OSVAULD_IOS_BUNDLE_ID"
