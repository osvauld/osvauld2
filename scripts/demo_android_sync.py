"""Desktop uploads/publishes; Android redeems an invite and proves two-way note sync.

Install the x86_64 adb-debug APK and boot osvauld-api35 first. Run:
    python3 scripts/demo_android_sync.py --adb ~/Android/Sdk/platform-tools/adb
Use --once for an automated run, --windowed to show the desktop. Requires root-enabled
emulator ADB. Creates throwaway accounts/stores; never clears existing Android app data.
"""

import argparse
import os
import subprocess
import time
from pathlib import Path
from tempfile import mkdtemp

import demo_sync
from osvauld.android import AndroidSession
from osvauld.session import Session

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--serial", default="emulator-5554")
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--windowed", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
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
        item = alice.rpc.create_item(ws["id"], "scratch", "app")
        clock.step("desktop uploads scratch", lambda: alice.rpc.upload_folder(item["id"], ROOT / "demo_apps/scratch"))
        alice.rpc.open_item(item["id"])
        clock.step("desktop publishes workspace", alice.rpc.publish_all)
        clock.step("desktop publishes app source", lambda: alice.rpc.push_src(item["id"]))
        demo_sync.add_note(alice, item["id"], "published from desktop")
        invite = clock.step("desktop issues invite", alice.rpc.invite)
        clock.step("Android debug sockets ready", android.start)
        account = "android-demo-" + directory.name.rsplit("-", 1)[-1]
        clock.step("Android throwaway signup", lambda: android.rpc.signup(account, "demo"))
        clock.step("Android redeems invite", lambda: android.rpc.claim_node(invite))
        clock.step("Android joins published item", lambda: android.rpc.join_item(ws, item))
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
