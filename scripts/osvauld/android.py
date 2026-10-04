"""Root-enabled emulator UDS forwarding; no TCP and no Android app-data reset."""

import os
import subprocess
import time
import warnings
from pathlib import Path

from .client import Bridge

PACKAGE = "com.osvauld.android"


class AndroidSession:
    def __init__(self, adb: str, serial: str, directory: Path, node_socket: Path):
        self.command = [adb, "-s", serial]
        self.local = directory / "android.sock"
        self.remote = f"/data/user/0/{PACKAGE}/files"
        self.node_socket = node_socket
        self.forwarded = self.reversed = self.started = False
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
        self.adb("root")
        self.adb("wait-for-device")
        if self.adb("shell", "id", "-u") != "0":
            raise RuntimeError("Emulator image does not permit root ADB")
        try:
            # Do not replace an existing mapping: another demo may own it.
            self.adb("reverse", "--no-rebind", f"localfilesystem:{self.remote}/kunki.sock",
                     f"localfilesystem:{self.node_socket}")
            self.reversed = True
            uid = self.adb("shell", "run-as", PACKAGE, "id", "-u")
            if not uid.isdigit():
                raise RuntimeError("Cannot resolve Android app UID")
            socket = f"{self.remote}/kunki.sock"
            self.adb("shell", "chown", f"{uid}:{uid}", socket)
            self.adb("shell", "chmod", "600", socket)
            self.adb("shell", "restorecon", "-F", socket)
            if self.adb("shell", "stat", "-c", "%a:%u", socket) != f"600:{uid}":
                raise RuntimeError("Android node socket permissions are incorrect")
            # Fresh process avoids carrying account-scoped worker replies from a prior session.
            self.adb("shell", "am", "force-stop", PACKAGE)
            self.started = True
            self.adb("shell", "am", "start", "-W", "-n", f"{PACKAGE}/android.app.NativeActivity")
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
        mappings = []
        if self.forwarded:
            mappings.append(("forward", f"localfilesystem:{self.local}"))
        if self.reversed:
            mappings.append(("reverse", f"localfilesystem:{self.remote}/kunki.sock"))
        for kind, endpoint in mappings:
            try:
                result = subprocess.run(self.command + [kind, "--remove", endpoint], capture_output=True, timeout=10)
                if result.returncode:
                    warnings.warn(f"ADB cleanup failed for {endpoint}")
            except (OSError, subprocess.TimeoutExpired) as error:
                warnings.warn(f"ADB cleanup failed: {error}")
        self.forwarded = self.reversed = False
