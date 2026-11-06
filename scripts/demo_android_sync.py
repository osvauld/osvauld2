"""Desktop hosts a space; Android joins it by the share link and proves two-way note sync.

Install the x86_64 adb-debug APK and boot osvauld-api35 first. Run:
    python3 scripts/demo_android_sync.py --adb ~/Android/Sdk/platform-tools/adb
Use --once for an automated run, --windowed to show the desktop. Requires root-enabled
emulator ADB (the debug bridge lives in the app's private files/). Creates throwaway
accounts/stores; never clears existing Android app data.

No kunki process: the desktop shell is the space's host (docs/design/laptop-host.md §2), run
with OSVAULD_NET=local so its link carries its direct addresses — loopback and the laptop's
LAN address — and no relay. Android joins by that link once and opens what the space lists,
by name; no id the script knew crosses to the phone. The ADB-forwarded node socket is gone
(`shell2::node::connect` refuses on Android); only the debug bridge `files/shell.sock` is
forwarded.

Ported to the host path 2026-11-03 (laptop-host step 8); not re-run on the emulator since —
the first run will need the emulator to reach the laptop's LAN address, which the link carries.

One-command demo: --boot starts the emulator if no device is up (-gpu host; SwiftShader lacks
the float16 capability Vello needs), --install builds and installs the x86_64 APK, and --apps
puts a set of demo apps in the space, all of which Android opens. `scratch` always goes first:
it is the item the two-way sync check runs on.
"""

import argparse
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from tempfile import mkdtemp

import demo_sync
from osvauld.android import AndroidSession
from osvauld.session import Session

ROOT = Path(__file__).resolve().parent.parent
AVD = "osvauld-api35"
DEFAULT_APPS = "scratch,pomodoro,tally,kanban,line_chart,pie,slides"


def device_up(adb, serial):
    out = subprocess.run([adb, "devices"], capture_output=True, text=True).stdout
    return any(l.split()[:2] == [serial, "device"] for l in out.splitlines())


def boot_emulator(adb, serial, headless):
    if device_up(adb, serial):
        print("Emulator already up", flush=True)
        return
    emulator = Path(os.environ.get("ANDROID_HOME", Path.home() / "Android/Sdk")) / "emulator/emulator"
    cmd = [str(emulator), "-avd", AVD, "-gpu", "host", "-writable-system", "-no-snapshot-save"]
    if headless:
        cmd += ["-no-window", "-no-audio"]
    subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    subprocess.run([adb, "-s", serial, "wait-for-device"], check=True, timeout=240)
    deadline = time.monotonic() + 240
    while time.monotonic() < deadline:
        done = subprocess.run([adb, "-s", serial, "shell", "getprop", "sys.boot_completed"],
                              capture_output=True, text=True).stdout.strip()
        if done == "1":
            break
        time.sleep(3)
    else:
        raise RuntimeError("emulator did not finish booting")
    subprocess.run([adb, "-s", serial, "root"], check=True)
    time.sleep(3)
    subprocess.run([adb, "-s", serial, "wait-for-device"], check=True, timeout=60)


def check_apps(android, adb, serial, items, outdir):
    """Per app: open it on the device, screenshot via adb, tap the screen centre, screenshot again.
    `items` are the records Android got from the host at join, in app order. The Android shell
    is windowed, so the offscreen-only driver ops (Frame, Rects) are not available; this is the
    real device path instead — adb pixels and adb touches."""
    outdir.mkdir(parents=True, exist_ok=True)

    def shot(path):
        png = subprocess.run([adb, "-s", serial, "exec-out", "screencap", "-p"],
                             capture_output=True, timeout=30).stdout
        path.write_bytes(png)
        return len(png)

    rows = []
    for it in items:
        name = it["name"]
        row = {"app": name}
        try:
            android.rpc.open_item(it["id"])
            time.sleep(1.5)
            row["shot1_bytes"] = shot(outdir / f"{name}-1.png")
            subprocess.run([adb, "-s", serial, "shell", "input", "tap", "540", "1200"], timeout=15)
            time.sleep(1.0)
            row["shot2_bytes"] = shot(outdir / f"{name}-2.png")
            console = android.rpc.read_console(it["id"], last=50)
            row["console_errors"] = [l for l in console if "error" in l.lower() or "panic" in l.lower()][:3]
        except Exception as e:  # a probe reports, it never aborts the rest
            row["failed"] = str(e)[:200]
        rows.append(row)
    return rows


def install_apk(adb, serial):
    subprocess.run([sys.executable, str(ROOT / "scripts/build_android.py"),
                    "--target", "x86_64-linux-android"], cwd=ROOT, check=True)
    subprocess.run([adb, "-s", serial, "install", "-r",
                    str(ROOT / "target/debug/apk/osvauld.apk")], check=True)


def build_shell():
    cmd = ["cargo", "build", "-p", "shell2"]
    # Through the task manager (AGENTS.md § Commands): a bare cargo puts a target/ on the home partition.
    if shutil.which("loadtable"):
        cmd = ["loadtable", "run", "--who", "android-demo", "--pin", "--", *cmd, "-j", "2"]
    subprocess.run(cmd, cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--serial", default="emulator-5554")
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--windowed", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--boot", action="store_true", help="start the emulator if none is up")
    parser.add_argument("--headless", action="store_true", help="with --boot: no emulator window")
    parser.add_argument("--install", action="store_true", help="build and install the x86_64 APK")
    parser.add_argument("--check", action="store_true", help="probe every app on Android; shots in shots/android-apps/")
    parser.add_argument("--apps", default=DEFAULT_APPS,
                        help="comma-separated demo_apps folders to put in the space; scratch is forced first")
    args = parser.parse_args()
    apps = ["scratch"] + [a for a in args.apps.split(",") if a and a != "scratch"]
    for a in apps:
        if not (ROOT / "demo_apps" / a).is_dir():
            parser.error(f"no demo app {a!r} in demo_apps/")
    if args.boot:
        boot_emulator(args.adb, args.serial, args.headless)
    if args.install:
        install_apk(args.adb, args.serial)
    if not args.skip_build:
        build_shell()
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    directory = Path(mkdtemp(prefix="osvauld-android-demo-"))
    print(f"Throwaway demo state: {directory}", flush=True)
    # The desktop shell hosts the space itself, as `Net.__enter__` sets it up: no node socket
    # (a shell still looking for one fails loudly), local reach only.
    previous = {k: os.environ.get(k) for k in ("OSVAULD_NET", "OSVAULD_KUNKI_SOCKET")}
    os.environ.pop("OSVAULD_KUNKI_SOCKET", None)
    os.environ["OSVAULD_NET"] = "local"
    if args.windowed:
        os.environ.pop("OSVAULD_OFFSCREEN", None)
    alice = Session(shell_binary=target / "debug" / "shell2", data_dir=str(directory / "alice"),
                    socket_path=str(directory / "alice.sock"),
                    offscreen=None if args.windowed else (900, 700))
    android = AndroidSession(args.adb, args.serial, directory)
    clock = demo_sync.Clock()
    try:
        clock.step("desktop ready", alice.start)
        clock.step("desktop signup", lambda: alice.rpc.signup("alice", "demo"))
        ws = alice.rpc.create_workspace("Android sync demo")
        hosting = alice.rpc.whoami().get("hosting") or []
        if ws["id"] not in hosting:
            raise RuntimeError(f"desktop does not host its own space: hosting={hosting}")
        print("desktop hosts the space from its own endpoint", flush=True)
        scratch = None
        for name in apps:
            it = alice.rpc.create_item(ws["id"], name, "app")
            clock.step(f"desktop uploads {name}", lambda it=it, name=name: alice.rpc.upload_folder(it["id"], ROOT / "demo_apps" / name))
            scratch = scratch or it
        # The open doc is the host's live copy; its edits fan out to sessions from there.
        alice.rpc.open_item(scratch["id"])
        demo_sync.add_note(alice, scratch["id"], "published from desktop")
        link = clock.step("desktop shares the space", lambda: alice.rpc.share_space(ws["id"]))
        print(f"share link: {link}", flush=True)
        clock.step("Android debug bridge ready", android.start)
        account = "android-demo-" + directory.name.rsplit("-", 1)[-1]
        clock.step("Android throwaway signup", lambda: android.rpc.signup(account, "demo"))
        # The link lists the host's direct addresses. Inside the emulator its 127.0.0.1 entry is
        # the guest itself; the laptop's private LAN address, also in the link, is the one the
        # guest reaches through the emulator's NAT.
        joined = clock.step("Android joins by link", lambda: android.rpc.join_space(link))
        by_name = {it["name"]: it for it in joined["items"]}
        missing = [n for n in apps if n not in by_name]
        if missing:
            raise RuntimeError(f"Android joined but the space lists no {missing}: {joined}")
        # Android's own records, in app order: the ids come from the host, never from this script.
        phone = [by_name[n] for n in apps]
        note = phone[0]["id"]
        android.rpc.open_item(note)
        demo_sync.wait_for_note(android, note, "published from desktop", 15)
        demo_sync.add_note(alice, scratch["id"], "live from desktop")
        clock.step("desktop edit reaches Android", lambda: demo_sync.wait_for_note(android, note, "live from desktop", 15))
        demo_sync.add_note(android, note, "live from Android")
        clock.step("Android edit reaches desktop", lambda: demo_sync.wait_for_note(alice, scratch["id"], "live from Android", 15))
        if args.check:
            for row in check_apps(android, args.adb, args.serial, phone, ROOT / "shots/android-apps"):
                print("CHECK", row, flush=True)
        print(f"Sync verified both ways. Android account: {account}, passphrase: demo", flush=True)
        print("Android app data was not erased. Ctrl-C stops the demo and removes its ADB mapping.", flush=True)
        if not args.once:
            while alice.process.poll() is None:
                time.sleep(0.5)
    except KeyboardInterrupt:
        pass
    finally:
        android.close()
        alice.close()
        for key, value in previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


if __name__ == "__main__":
    main()
