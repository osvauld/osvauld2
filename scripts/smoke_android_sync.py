"""Opt-in smoke: installed adb-debug APK, booted root-enabled emulator, real two-way sync over
the host path — the desktop shell hosts the space, Android joins by its share link (no kunki,
no ADB-forwarded node socket; laptop-host step 8). OSVAULD_ANDROID_SMOKE=1 runs
demo_android_sync.py --once; the emulator must reach the laptop's LAN address, which the link
carries."""
import os
import subprocess
import sys
from pathlib import Path

if os.environ.get("OSVAULD_ANDROID_SMOKE") != "1":
    print("Android sync smoke skipped (set OSVAULD_ANDROID_SMOKE=1 with emulator/APK ready)")
else:
    sdk = os.environ.get("ANDROID_HOME")
    adb = os.environ.get("OSVAULD_ADB") or (str(Path(sdk) / "platform-tools/adb") if sdk else "adb")
    raise SystemExit(subprocess.run([
        sys.executable, str(Path(__file__).with_name("demo_android_sync.py")),
        "--once", "--adb", adb,
        "--serial", os.environ.get("OSVAULD_ANDROID_SERIAL", "emulator-5554"),
    ]).returncode)
