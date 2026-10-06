"""Desktop uploads/publishes; Android redeems an invite and proves two-way note sync.

Install the x86_64 adb-debug APK and boot osvauld-api35 first. Run:
    python3 scripts/demo_android_sync.py --adb ~/Android/Sdk/platform-tools/adb
Use --once for an automated run, --windowed to show the desktop. Requires root-enabled
emulator ADB. Creates throwaway accounts/stores; never clears existing Android app data.

One-command demo: --boot starts the emulator if no device is up (-gpu host; SwiftShader lacks
the float16 capability Vello needs), --install builds and installs the x86_64 APK, and --apps
publishes a set of demo apps, all of which Android joins. `scratch` always goes first: it is the
item the two-way sync check runs on.
"""

import argparse
import os
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


def install_apk(adb, serial):
    subprocess.run([sys.executable, str(ROOT / "scripts/build_android.py"),
                    "--target", "x86_64-linux-android"], cwd=ROOT, check=True)
    subprocess.run([adb, "-s", serial, "install", "-r",
                    str(ROOT / "target/debug/apk/osvauld.apk")], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--serial", default="emulator-5554")
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--windowed", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--boot", action="store_true", help="start the emulator if none is up")
    parser.add_argument("--headless", action="store_true", help="with --boot: no emulator window")
    parser.add_argument("--install", action="store_true", help="build and install the x86_64 APK")
    parser.add_argument("--apps", default=DEFAULT_APPS,
                        help="comma-separated demo_apps folders to publish; scratch is forced first")
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
        subprocess.run(["cargo", "build", "-p", "shell2", "-p", "kunki"], cwd=ROOT, check=True)
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    demo_sync.KUNKI_BINARY = target / "debug" / "kunki"
    directory = Path(mkdtemp(prefix="osvauld-android-demo-"))
    print(f"Throwaway demo state: {directory}", flush=True)
    node, socket, ticket = demo_sync.spawn_node(directory)
    previous_socket = os.environ.get("OSVAULD_KUNKI_SOCKET")
    os.environ["OSVAULD_KUNKI_SOCKET"] = str(socket)
    if args.windowed:
        os.environ.pop("OSVAULD_OFFSCREEN", None)
    alice = Session(shell_binary=target / "debug" / "shell2", data_dir=str(directory / "alice"),
                    socket_path=str(directory / "alice.sock"),
                    offscreen=None if args.windowed else (900, 700))
    android = AndroidSession(args.adb, args.serial, directory, socket)
    clock = demo_sync.Clock()
    try:
        clock.step("desktop ready", alice.start)
        clock.step("desktop signup", lambda: alice.rpc.signup("alice", "demo"))
        clock.step("desktop claims node", lambda: alice.rpc.claim_node(ticket))
        ws = alice.rpc.create_workspace("Android sync demo")
        items = []
        for name in apps:
            it = alice.rpc.create_item(ws["id"], name, "app")
            clock.step(f"desktop uploads {name}", lambda it=it, name=name: alice.rpc.upload_folder(it["id"], ROOT / "demo_apps" / name))
            alice.rpc.open_item(it["id"])
            items.append(it)
        item = items[0]
        clock.step("desktop publishes workspace", alice.rpc.publish_all)
        for it in items:
            clock.step(f"desktop publishes source {it['id'][:6]}", lambda it=it: alice.rpc.push_src(it["id"]))
        demo_sync.add_note(alice, item["id"], "published from desktop")
        invite = clock.step("desktop issues invite", alice.rpc.invite)
        clock.step("Android debug sockets ready", android.start)
        account = "android-demo-" + directory.name.rsplit("-", 1)[-1]
        clock.step("Android throwaway signup", lambda: android.rpc.signup(account, "demo"))
        clock.step("Android redeems invite", lambda: android.rpc.claim_node(invite))
        for it in items:
            clock.step(f"Android joins {it['id'][:6]}", lambda it=it: android.rpc.join_item(ws, it))
        android.rpc.open_item(item["id"])
        demo_sync.wait_for_note(android, item["id"], "published from desktop", 15)
        demo_sync.add_note(alice, item["id"], "live from desktop")
        clock.step("desktop edit reaches Android", lambda: demo_sync.wait_for_note(android, item["id"], "live from desktop", 15))
        demo_sync.add_note(android, item["id"], "live from Android")
        clock.step("Android edit reaches desktop", lambda: demo_sync.wait_for_note(alice, item["id"], "live from Android", 15))
        print(f"Sync verified both ways. Android account: {account}, passphrase: demo", flush=True)
        print("Android app data was not erased. Ctrl-C stops the demo and removes its ADB mappings.", flush=True)
        if not args.once:
            while alice.process.poll() is None and node.poll() is None:
                time.sleep(0.5)
    except KeyboardInterrupt:
        pass
    finally:
        android.close()
        alice.close()
        try:
            node.terminate()
            try:
                node.wait(timeout=10)
            except subprocess.TimeoutExpired:
                node.kill()
                node.wait(timeout=10)
        finally:
            if previous_socket is None:
                os.environ.pop("OSVAULD_KUNKI_SOCKET", None)
            else:
                os.environ["OSVAULD_KUNKI_SOCKET"] = previous_socket


if __name__ == "__main__":
    main()
