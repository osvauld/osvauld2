"""Opt-in native multi-touch and background/resume on an installed root-enabled debug emulator.

OSVAULD_JETPACK_ANDROID=1 OSVAULD_ANDROID_SERIAL=emulator-5580 python3 scripts/smoke_jetpack_android.py
Uses a new account, never clears app data. Run in a disposable/read-only AVD.
"""
import os
import re
import struct
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

from osvauld.client import Bridge, BridgeError


def main():
    if os.environ.get("OSVAULD_JETPACK_ANDROID") != "1":
        print("jetpack Android: skipped (set OSVAULD_JETPACK_ANDROID=1 on a disposable emulator)")
        return
    serial = os.environ.get("OSVAULD_ANDROID_SERIAL", "emulator-5580")
    adb_path = os.environ.get("ADB", str(Path.home() / "Android/Sdk/platform-tools/adb"))
    command = [adb_path, "-s", serial]

    def adb(*args, data=None):
        result = subprocess.run(command + list(args), input=data, capture_output=True, timeout=30)
        if result.returncode:
            raise RuntimeError(result.stderr.decode(errors="replace"))
        return result.stdout.decode(errors="replace").strip()

    assert adb("shell", "getprop", "ro.kernel.qemu") == "1", "requires a disposable emulator"
    assert adb("shell", "id", "-u") == "0", "requires root-enabled ADB"
    display = adb("shell", "wm", "size")
    width, height = map(int, re.findall(r"(\d+)x(\d+)", display)[-1])
    density = int(re.findall(r"\d+", adb("shell", "wm", "density"))[-1]) / 160
    assert width < height, "native harness currently expects portrait sensor coordinates"
    devices = adb("shell", "getevent", "-lp")
    touch = re.search(r"add device \d+: (/dev/input/event\d+)\s+name:\s+\"virtio_input_multi_touch_1\"", devices)
    assert touch, "unrecognized emulator touch device"
    device = touch[1]
    active = set()

    def finger(slot, phase, x=0, y=0):
        events = [(3, 47, slot)]
        if phase == "start":
            events += [(3, 57, slot + 100), (3, 55, 0), (3, 58, 900), (3, 48, 8)]
            active.add(slot)
        if phase == "end":
            events += [(3, 57, -1)]
            active.discard(slot)
        else:
            events += [(3, 53, round(x * density / (width - 1) * 32767)),
                       (3, 54, round(y * density / (height - 1) * 32767))]
        events += [(0, 0, 0)]
        packed = b"".join(struct.pack("llHHi", 0, 0, *event) for event in events)
        adb("shell", f"cat > {device}", data=packed)

    with tempfile.TemporaryDirectory(prefix="jetpack-android-") as directory:
        socket = str(Path(directory) / "shell.sock")
        package = "com.osvauld.android"
        remote = f"/data/user/0/{package}/files/shell.sock"
        forwarded = False
        try:
            adb("shell", "am", "force-stop", package)
            adb("shell", "am", "start", "-W", "-n", f"{package}/android.app.NativeActivity")
            adb("forward", "--no-rebind", f"localfilesystem:{socket}", f"localfilesystem:{remote}")
            forwarded = True
            os.chmod(socket, 0o600)
            r = Bridge(socket)
            deadline = time.monotonic() + 40
            while True:
                try:
                    if r.ping() == "pong":
                        break
                except (OSError, ConnectionError):
                    pass
                assert time.monotonic() < deadline, "Android shell bridge did not start"
                time.sleep(0.1)
            r.signup("pilot-" + uuid.uuid4().hex[:8], "temporary jetpack emulator passphrase")
            ws = r.create_workspace("Jetpack native smoke")
            item = r.create_item(ws["id"], "Jetpack Arena", "app")["id"]
            r.upload_folder(item, Path(__file__).resolve().parent.parent / "demo_apps/jetpack")
            r.open_item(item)

            def labels(tree):
                found = {tree["id"]: tree.get("text")} if tree.get("id") else {}
                for child in tree.get("children", []):
                    found.update(labels(child))
                return found

            def number(identity):
                return int(labels(r.dump_tree(item))[identity].split()[-1])

            def centre(identity):
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    try:
                        e = next((e for e in r.rects() if e["id"] == identity), None)
                        if e:
                            return e["x"] + e["w"] / 2, e["y"] + e["h"] / 2
                    except BridgeError as error:
                        if "layout changed" not in str(error):
                            raise
                    time.sleep(0.03)
                raise AssertionError(f"native control geometry unavailable: {identity}")

            r.click(item, "match:start")
            mx, my = centre("control:move")
            ax, ay = centre("control:aim")
            bx, by = centre("control:boost")
            finger(0, "start", mx, my)
            finger(0, "move", mx + 30, my)
            finger(1, "start", ax, ay)
            finger(1, "move", ax + 30, ay - 5)
            finger(2, "start", bx, by)
            time.sleep(0.7)
            assert number("hud:fuel") < 95, "native thrust finger was not held"
            assert number("hud:altitude") > 20, "native thrust did not lift player"
            assert number("hud:fired") > 0, "native aim/fire did not run beside movement/thrust"
            r.save_screenshot(item, "/tmp/jetpack-android-native.png")
            px, py = centre("match:pause")
            finger(3, "start", px, py)
            finger(3, "end")
            assert labels(r.dump_tree(item))["match:state"] == "Paused", "held fingers blocked native pause"
            for slot in list(active):
                finger(slot, "end")
            r.click(item, "match:resume")
            # Background with held aim/thrust; foregrounding must not restore stale input.
            finger(1, "start", ax, ay)
            finger(1, "move", ax + 30, ay)
            finger(2, "start", bx, by)
            time.sleep(0.2)
            adb("shell", "input", "keyevent", "KEYCODE_HOME")
            for slot in list(active):
                finger(slot, "end")
            time.sleep(0.2)
            adb("shell", "am", "start", "-W", "-n", f"{package}/android.app.NativeActivity")
            time.sleep(0.3)
            fired = number("hud:fired")
            time.sleep(0.3)
            assert number("hud:fired") == fired, "background/resume retained held fire"
            assert not r.read_console(item), r.read_console(item)
            print("jetpack Android: native three-finger flight/fire, held-control pause and resume passed")
        finally:
            for slot in list(active):
                finger(slot, "end")
            if forwarded:
                adb("forward", "--remove", f"localfilesystem:{socket}")
            adb("shell", "am", "force-stop", package)


if __name__ == "__main__":
    main()
