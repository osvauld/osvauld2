"""e2e_host.py — docs/design/laptop-host.md §0, end to end.

    python3 scripts/e2e_host.py            # every test
    python3 scripts/e2e_host.py h2 h3      # just these

Two or three offscreen shells per test, no kunki process, no kunki socket: alice's shell hosts
the space from its own device endpoint and the others dial it over iroh on loopback
(`OSVAULD_NET=local`, see osvauld/net.py). Screenshots land in $OSVAULD_SHOTS (default
target/e2e-shots) as `H<n>-<peer>.png`; a step is judged on those as well as the assertions.
OSVAULD_KEEP=1 keeps every vault directory for a post-mortem.

The chat app is the artifact throughout: it already has a composer, a message list and a
manifest with roles, so "A's data" is a message and "both edit" is both posting.

Rust tests carry the halves a shell cannot drive: a 70 KiB first frame dropped before it is
read (`net`), three wrong codes closing one connection (`net`), an attestation for another DID
refused at `Hello` (`courier`). H5 here covers the halves a shell can.
"""

import os
import shutil
import subprocess
import sys
import time
import traceback
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.net import Net, build, wait_until  # noqa: E402
from e2e_chat_sync import CHAT, PUSH, send, sees, texts, shots  # noqa: E402

HERE = Path(__file__).parent
KUNKI_DEFAULT_SOCKET = Path("/tmp/osvauld-kunki.sock")


class Skip(Exception):
    """A test that needs hardware this run does not have."""


def refused(call, *words):
    """`call` must fail with a bridge error naming every word in `words`."""
    try:
        got = call()
    except Exception as e:  # noqa: BLE001 — the error text is the assertion
        text = str(e).lower()
        missing = [w for w in words if w.lower() not in text]
        assert not missing, f"refused, but not for {missing}: {e}"
        return
    raise AssertionError(f"accepted, should have been refused: {got}")


B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58decode(text: str) -> bytes:
    n = 0
    for ch in text:
        n = n * 58 + B58.index(ch)
    out = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    return b"\0" * (len(text) - len(text.lstrip("1"))) + out


def b58encode(data: bytes) -> str:
    n = int.from_bytes(data, "big")
    out = ""
    while n:
        n, r = divmod(n, 58)
        out = B58[r] + out
    return "1" * (len(data) - len(data.lstrip(b"\0"))) + out


def wrong_code(link: str) -> str:
    """The same link — same host, same addresses — with one byte of its 12-byte code
    changed: a code the host never issued. Layout: version(1) device(32) code(12) …"""
    raw = bytearray(b58decode(link[len("osvauld://s/"):]))
    raw[33] ^= 0x01
    return "osvauld://s/" + b58encode(bytes(raw))


def pixel_adb() -> list[str]:
    """`[adb, -s, SERIAL]` for the one attached phone; a Skip says what is missing. H8 is for
    an unrooted phone, so this never runs `adb root`."""
    adb = (os.environ.get("ADB") or shutil.which("adb")
           or str(Path.home() / "Android/Sdk/platform-tools/adb"))
    if not shutil.which(adb):
        raise Skip(f"no adb ({adb}); set $ADB")
    serial = os.environ.get("OSVAULD_ANDROID_SERIAL")
    if not serial:
        out = subprocess.run([adb, "devices"], capture_output=True, text=True).stdout
        attached = [line.split("\t")[0] for line in out.splitlines() if line.endswith("\tdevice")]
        if not attached:
            raise Skip("no device attached")
        if len(attached) > 1:
            raise Skip(f"{len(attached)} devices attached; set OSVAULD_ANDROID_SERIAL to one of "
                       + " ".join(attached))
        serial = attached[0]
    return [adb, "-s", serial]


def pixel_logcat(adb: list[str], lines: int = 40) -> str:
    """The debug APK's stderr lands in logcat under tag `osvauld` (android_app/src/lib.rs)."""
    r = subprocess.run([*adb, "logcat", "-d", "-s", "osvauld"], capture_output=True, text=True)
    return "\n".join(r.stdout.splitlines()[-lines:])


# ── tests ────────────────────────────────────────────────────────────────────


def h1(net):
    """The host is the app: A serves its space from its own device — no process, no socket."""
    stale = KUNKI_DEFAULT_SOCKET.stat().st_mtime if KUNKI_DEFAULT_SOCKET.exists() else None
    a = net.peer("alice")
    ws = a.rpc.create_workspace("team")
    who = a.rpc.whoami()
    assert who.get("device"), f"whoami names no device endpoint: {who}"
    assert ws["id"] in (who.get("hosting") or []), f"alice does not host her own space: {who}"
    hosts = a.rpc.hosts(ws["id"])
    assert [h["did"] for h in hosts] == [who["did"]], f"hosts should be alice alone: {hosts}"
    assert hosts[0]["device"] == who["device"] and hosts[0]["online"], hosts
    assert not (net.root / "kunki.sock").exists(), "a node socket appeared in the net dir"
    now = KUNKI_DEFAULT_SOCKET.stat().st_mtime if KUNKI_DEFAULT_SOCKET.exists() else None
    assert now == stale, "the shell touched the kunki default socket"
    a.rpc.save_shell_screenshot(net.shots / "H1-alice.png")


def h2(net):
    """Share and join: B joins by link, finds the app without any id, sees what A made."""
    a, b = net.peer("alice"), net.peer("bob")
    ws = a.rpc.create_workspace("team")
    item = a.rpc.create_item(ws["id"], CHAT.name, "app")
    a.rpc.upload_folder(item["id"], CHAT)
    a.rpc.open_item(item["id"])
    i = item["id"]
    send(a, i, "made before bob joined")
    link = a.rpc.share_space(ws["id"])
    assert link.startswith("osvauld://s/"), link
    # ~75 chars through a relay; a local link also carries the host's direct addresses.
    assert len(link) < 200, f"a link is typed on a phone; this one is {len(link)} chars"
    joined = b.rpc.join_space(link)
    assert joined["ws"]["name"] == "team", joined
    names = [it["name"] for it in joined["items"]]
    assert names == [CHAT.name], f"bob sees {names}, should see the one app"
    assert joined["items"][0]["id"] == i, "the item keeps its identity across peers"
    b.rpc.open_item(i)
    sees(b, i, "made before bob joined")
    shots("H2", i, a, b)


def h3(net):
    """Live both ways: each sees the other's post without a request of its own."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item, _ = net.share_space(a, [b], CHAT)
    i = item["id"]
    send(a, i, "hello from alice")
    sees(b, i, "hello from alice")
    send(b, i, "hello from bob")
    sees(a, i, "hello from bob")
    shots("H3", i, a, b)


def h4(net):
    """Host offline: B keeps editing, says so, and both converge once A is back."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item, _ = net.share_space(a, [b], CHAT)
    i = item["id"]
    send(a, i, "before alice slept")
    sees(b, i, "before alice slept")
    a.stop()
    send(b, i, "while alice slept")
    # The transport notices a vanished host by its idle timeout (10 s), not at once.
    wait_until(
        lambda: any("host offline" in line for line in b.rpc.read_console(i)),
        PUSH * 4, "bob's console to say the host is offline",
    )
    assert "while alice slept" in texts(b, i), "bob's own post must stay local while offline"
    a.restart()
    a.rpc.open_item(i)
    sees(a, i, "while alice slept", timeout=PUSH * 4)
    send(a, i, "alice is back")
    sees(b, i, "alice is back")
    assert texts(a, i) == texts(b, i), f"order differs:\n{texts(a, i)}\n{texts(b, i)}"
    shots("H4", i, a, b)


def h5(net):
    """Peer-facing guards: forged, exhausted and expired links refused; clock skew tolerated."""
    a, b, c = net.peer("alice"), net.peer("bob"), net.peer("carol")
    ws, item, link = net.share_space(a, [b], CHAT)
    i = item["id"]
    send(a, i, "for members only")
    bad = wrong_code(link)
    for _ in range(3):
        refused(lambda: c.rpc.join_space(bad), "refused")
    assert not c.rpc.list_workspaces(), "a refused join must adopt nothing"
    # Three wrong codes did not lock the right one out (that rule is per connection).
    single = a.rpc.share_space(ws["id"])
    # The host's clock moves past the link's week; the link it minted is dead, a fresh one lives.
    a.rpc.advance(8 * 86400)
    refused(lambda: c.rpc.join_space(single), "expired")
    fresh = a.rpc.share_space(ws["id"])
    # Carol's clock is five minutes ahead of the host's: still fine.
    c.rpc.advance(300)
    net.join_space(c, fresh, CHAT.name)
    sees(c, i, "for members only")
    send(c, i, "carol, five minutes early")
    sees(a, i, "carol, five minutes early")
    shots("H5", i, a, c)


def h6(net):
    """No regression: the chat sync tests pass over sessions with no node."""
    names = "t1 t2 t3 t4 t5 t8 t13 t15 t23a t8a t10a t15a t20a".split()
    env = {**os.environ, "OSVAULD_E2E_HOST": "1"}
    r = subprocess.run([sys.executable, str(HERE / "e2e_chat_sync.py"), *names], env=env)
    assert r.returncode == 0, "e2e_chat_sync.py failed in host mode (see its output above)"


def h7(net):
    """An appointed headless host carries the space while A is offline."""
    a, b, c = net.peer("alice"), net.peer("bob"), net.peer("carol")
    ws, item, link = net.share_space(a, [b, c], CHAT)
    i = item["id"]
    k = net.headless_host("kunki", link, ws["id"])
    a.rpc.appoint_host(ws["id"], k.did)
    wait_until(
        lambda: any(h["did"] == k.did and h["online"] for h in a.rpc.hosts(ws["id"])),
        PUSH, "kunki to come up as a host",
    )
    wait_until(
        lambda: any(h["did"] == k.did for h in b.rpc.hosts(ws["id"])),
        PUSH, "bob to learn the new host",
    )
    a.stop()
    send(b, i, "through kunki")
    # A's death is noticed by QUIC's idle timeout (10 s); then each member dials kunki at once.
    sees(c, i, "through kunki", timeout=PUSH * 4)
    assert not any("host offline" in line for line in b.rpc.read_console(i)), \
        "bob thought the space was offline with kunki hosting it"
    a.restart()
    a.rpc.open_item(i)
    sees(a, i, "through kunki", timeout=PUSH * 4)
    shots("H7", i, a, b, c)


def h8(net):
    """A Pixel joins by a typed link over iroh — no ADB socket — and plays the edit back.

    The phone has no bridge (an unrooted phone cannot forward an app-private unix socket), so a
    person at the Pixel does its half: this drives the laptop and talks to them through the
    terminal. adb only types the link, reads logcat and takes the screenshot."""
    if os.environ.get("OSVAULD_ANDROID_E2E") != "1":
        raise Skip("needs a connected Pixel with the debug APK; set OSVAULD_ANDROID_E2E=1")
    if not sys.stdin.isatty():
        raise Skip("needs a person at the Pixel: stdin is not a terminal")
    adb = pixel_adb()

    def pixel(*args, **kw):
        return subprocess.run([*adb, *args], check=True, **kw)

    def say(*lines):
        print(*lines, sep="\n", flush=True)

    a = net.peer("alice")
    ws = a.rpc.create_workspace("team")
    item = a.rpc.create_item(ws["id"], CHAT.name, "app")
    a.rpc.upload_folder(item["id"], CHAT)
    a.rpc.open_item(item["id"])
    i = item["id"]
    send(a, i, "made on the laptop")
    link = a.rpc.share_space(ws["id"])

    say("",
        "H8: the laptop and the Pixel must share a Wi-Fi.",
        "On the Pixel open Osvauld (the debug APK), sign up or unlock, tap `join`, then tap",
        "the box so the keyboard shows. The link:",
        "",
        f"    {link}",
        "")
    answer = input("Press Enter here and adb types the link into the box, "
                   "or type `done` if you typed it yourself: ")
    if answer.strip().lower() != "done":
        # `input text` goes through the device shell; the link has no shell-special
        # characters, quoted all the same. 66 = KEYCODE_ENTER, the panel's only submit.
        pixel("shell", "input", "text", f"'{link}'")
        pixel("shell", "input", "keyevent", "66")
    say("",
        "On the Pixel the space `team` appears. Open it, open `chat`, and post a message",
        "containing the word `pixel`.",
        "")
    try:
        wait_until(lambda: any("pixel" in t.lower() for t in texts(a, i)),
                   240, "the Pixel's post to reach the laptop", every=0.5)
        send(a, i, "seen on the laptop")
        answer = input("Does the Pixel now show 'seen on the laptop'? [y/N] ")
        assert answer.strip().lower().startswith("y"), \
            "the laptop's post did not play back on the Pixel"
    except AssertionError as e:
        raise AssertionError(f"{e}\n--- pixel logcat ---\n{pixel_logcat(adb)}") from None
    laptop = a.shot("H8", i)
    phone = net.shots / "H8-pixel.png"
    phone.write_bytes(pixel("exec-out", "screencap", "-p", capture_output=True).stdout)
    say(f"laptop: {laptop}", f"pixel:  {phone}")


def h9(net):
    """The product path: make, share, a friend joins, both edit, the friend comes back later."""
    import e2e_artifact
    e2e_artifact.story(net)


TESTS = {f"h{n}": globals()[f"h{n}"] for n in range(1, 10)}


def main() -> int:
    names = [a for a in sys.argv[1:] if not a.startswith("-")] or list(TESTS)
    build()
    failed, skipped = [], []
    for name in names:
        started = time.monotonic()
        try:
            with Net() as net:
                TESTS[name](net)
            print(f"ok      {name}  {TESTS[name].__doc__.splitlines()[0]}  "
                  f"({time.monotonic() - started:.1f}s)", flush=True)
        except Skip as e:
            skipped.append(name)
            print(f"skipped {name}  {e}", flush=True)
        except Exception as e:  # noqa: BLE001 — every failure is reported, the run goes on
            failed.append(name)
            print(f"FAILED  {name}  {type(e).__name__}: {e}", flush=True)
            if "-v" in sys.argv or len(names) == 1:
                traceback.print_exc()
    print(f"\n{len(names) - len(failed) - len(skipped)}/{len(names)} passed" +
          (f"; failed: {' '.join(failed)}" if failed else "") +
          (f"; skipped: {' '.join(skipped)}" if skipped else ""))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
