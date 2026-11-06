"""Root-enabled emulator forwarding of the debug bridge (`files/shell.sock`); no TCP, no node socket, no Android app-data reset."""

import os
import subprocess
import time
import warnings
from pathlib import Path

from .client import Bridge

PACKAGE = "com.osvauld.android"


class AndroidSession:
    def __init__(self, adb: str, serial: str, directory: Path):
        self.command = [adb, "-s", serial]
        self.local = directory / "android.sock"
        self.remote = f"/data/user/0/{PACKAGE}/files"
        self.forwarded = self.started = False
        self.rpc = Bridge(str(self.local))

    def adb(self, *args: str) -> str:
        result = subprocess.run(self.command + list(args), capture_output=True, text=True, timeout=30)
        if result.returncode:
            raise RuntimeError(f"ADB {args[0]} failed: {result.stderr.strip()}")
        return result.stdout.strip()

    def start(self):
        if self.adb("shell", "getprop", "ro.kernel.qemu") != "1":
            raise RuntimeError("This demo requires a root-enabled debug emulator, not a physical device")
        if self.local.parent.stat().st_mode & 0o077:
            raise RuntimeError("Host forwarding directory must be private (0700)")
        # Root is what lets adbd reach a socket inside the app's private files directory.
        self.adb("root")
        self.adb("wait-for-device")
        if self.adb("shell", "id", "-u") != "0":
            raise RuntimeError("Emulator image does not permit root ADB")
        try:
            # Fresh process avoids carrying account-scoped worker replies from a prior session.
            self.adb("shell", "am", "force-stop", PACKAGE)
            self.started = True
            self.adb("shell", "am", "start", "-W", "-n", f"{PACKAGE}/android.app.NativeActivity")
            # Do not replace an existing mapping: another demo may own it.
            self.adb("forward", "--no-rebind", f"localfilesystem:{self.local}",
                     f"localfilesystem:{self.remote}/shell.sock")
            self.forwarded = True
            os.chmod(self.local, 0o600)
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                try:
                    if self.rpc.ping() == "pong":
                        return self
                except (OSError, ConnectionError):
                    pass
                time.sleep(0.1)
            raise RuntimeError("Android debug bridge unavailable; install the adb-debug APK first")
        except BaseException:
            self.close()
            raise

    def close(self):
        if self.started:
            try:
                self.adb("shell", "am", "force-stop", PACKAGE)
            except (RuntimeError, OSError, subprocess.TimeoutExpired) as error:
                warnings.warn(f"Could not stop Android demo: {error}")
            self.started = False
        if self.forwarded:
            endpoint = f"localfilesystem:{self.local}"
            try:
                result = subprocess.run(self.command + ["forward", "--remove", endpoint], capture_output=True, timeout=10)
                if result.returncode:
                    warnings.warn(f"ADB cleanup failed for {endpoint}")
            except (OSError, subprocess.TimeoutExpired) as error:
                warnings.warn(f"ADB cleanup failed: {error}")
        self.forwarded = False
