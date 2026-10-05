"""App threads, end to end: docs/design/app-threads.md §0, T1–T11.

    python3 scripts/e2e_app_threads.py            # all
    python3 scripts/e2e_app_threads.py T1 T5      # some

Each test gets fresh offscreen shells and reports PASS/FAIL without stopping the run — most
fail until their step lands. Wall-clock limits measure bridge round trips, never the virtual
clock. The probe app's `__busy`/`__stall` are debug-only test bindings (`app_host`
`install_test_bindings`) standing in for native work the interrupt budget cannot see.
"""

import os
import subprocess
import sys
import tempfile
import threading
import time
import traceback
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.client import Bridge, BridgeError  # noqa: E402
from osvauld.session import ROOT, Session, build_shell  # noqa: E402

os.environ["OSVAULD_TEST_BINDINGS"] = "1"
os.environ.setdefault("OSVAULD_OFFSCREEN", "900x700")

KUNKI = ROOT / "target" / "debug" / "kunki"
# What "the chrome and other tiles stay live" means, per round trip. A debug build answers B in
# ~30-45 ms with A idle (each request also paints); A's "busy" is 300 ms, so a request that
# waited on A would take up to that. Half of it separates the two. (Was 0.05, set before any
# baseline existed; revised 2026-10-04.)
RESPONSIVE = 0.15

# One app, every behaviour a test needs behind a button. `view` logs a line per run so a
# test can see an app's Lua run without asking it to (asking would run it).
PROBE = """
local d = doc:open("probe")
local heavy = false
local hoard = {}
local views = 0
local ticks = 0

local function notes()
	local n = 0
	for k in pairs(d) do
		if type(k) == "string" and string.sub(k, 1, 5) == "note:" then
			n += 1
		end
	end
	return n
end

local function btn(id, f)
	return ui.button({ id = id, h = 24, px = 8, ui.text({ id, no_wrap = true }), on_click = f })
end

return function()
	views += 1
	__log("probe-view " .. views .. " " .. notes())
	if heavy then
		__busy(40)
	end
	return ui.col({
		id = "probe",
		gap = 4,
		-- heavy: a view of 40 ms on every frame, which only a declared on_frame asks for
		on_frame = heavy and function() ticks += 1 end or nil,
		ui.text({ "PROBE", id = "label", no_wrap = true }),
		btn("add", function() d:set({ "note:" .. uuid() }, doc.map({ text = "hi" })) end),
		btn("busy", function() __busy(300) end),
		btn("spin", function() for _ = 1, 1000 do __busy(10) end end),
		btn("hog", function() for i = 1, 16 do hoard[i] = string.rep("x", 8 * 1024 * 1024) .. i end end),
		btn("stall", function() __stall() end),
		btn("heavy", function() heavy = true end),
	})
end
"""


class Shell:
    """A Session whose stdout is kept, so `print` from a view can be read back."""

    def __init__(self, tmp: Path, name: str, **kw):
        self.log = tmp / f"{name}.out"
        self.session = Session(stdout_path=str(self.log), **kw)
        self.rpc = self.session.rpc

    def __enter__(self):
        self.session.start()
        return self

    def __exit__(self, *_):
        self.session.close()

    def fast(self, timeout: float = 2.0) -> Bridge:
        return Bridge(self.session.socket_path, timeout=timeout)

    def alive(self) -> bool:
        return self.session.process is not None and self.session.process.poll() is None

    def views(self) -> list[str]:
        if not self.log.exists():
            return []
        return [l for l in self.log.read_text(errors="replace").splitlines() if l.startswith("probe-view")]


def probe_item(rpc, ws_id: str, name: str) -> str:
    item = rpc.create_item(ws_id, name, "app")["id"]
    rpc.write_file(item, "main.lua", PROBE)
    rpc.open_item(item)
    return item


def two_probes(sh: Shell) -> tuple[str, str]:
    sh.rpc.signup("abe", "test")
    ws = sh.rpc.create_workspace("threads")["id"]
    a = probe_item(sh.rpc, ws, "a")
    b = probe_item(sh.rpc, ws, "b")
    sh.rpc.frame(2)
    return a, b


def timed(f) -> float:
    t0 = time.monotonic()
    f()
    return time.monotonic() - t0


def expect_error(rpc, item: str, words: str) -> None:
    try:
        rpc.dump_tree(item)
    except BridgeError as e:
        assert words in str(e), e
        return
    raise AssertionError(f"DumpTree({item}) answered; expected {words!r}")


def background(f) -> threading.Thread:
    t = threading.Thread(target=f, daemon=True)
    t.start()
    return t


def worst_while(busy: threading.Thread, checks) -> float:
    """The slowest round trip among `checks`, run in a loop for as long as `busy` runs."""
    worst = 0.0
    time.sleep(0.05)
    while busy.is_alive():
        for c in checks:
            worst = max(worst, timed(c))
    return worst


def console_has(rpc, item: str, word: str) -> bool:
    return any(word in line.lower() for line in rpc.read_console(item))


TESTS = {}


def test(f):
    TESTS[f.__name__.split("_")[0].upper()] = f
    return f


@test
def t1_slow_tile(tmp):
    with Shell(tmp, "t1") as sh:
        a, b = two_probes(sh)
        fast = sh.fast()
        busy = background(lambda: [sh.rpc.click(a, "busy") for _ in range(8)])
        worst = worst_while(busy, [lambda: fast.dump_tree(b), lambda: fast.open_item(b)])
        assert worst < RESPONSIVE, f"B / chrome took {worst * 1000:.0f} ms while A was busy"


@test
def t2_wall_budget(tmp):
    os.environ["OSVAULD_APP_BUDGET_MS"] = "500"
    with Shell(tmp, "t2") as sh:
        a, b = two_probes(sh)
        fast = sh.fast()
        spent = []
        spin = background(lambda: spent.append(timed(lambda: sh.rpc.click(a, "spin"))))
        worst = worst_while(spin, [lambda: fast.dump_tree(b)])
        assert spent and spent[0] < 2.0, f"spin ran {spent[0] if spent else '?'} s; budget is 0.5 s"
        assert console_has(sh.rpc, a, "budget"), sh.rpc.read_console(a)
        assert worst < RESPONSIVE, f"B took {worst * 1000:.0f} ms while A spun"
        assert sh.alive()


@test
def t3_memory_cap(tmp):
    os.environ["OSVAULD_APP_MEMORY_MB"] = "32"
    with Shell(tmp, "t3") as sh:
        a, b = two_probes(sh)
        sh.rpc.click(a, "hog")
        assert console_has(sh.rpc, a, "memory"), sh.rpc.read_console(a)
        assert sh.alive()
        sh.fast().dump_tree(b)


@test
def t4_stuck_thread(tmp):
    os.environ["OSVAULD_WATCHDOG_MS"] = "1000"
    with Shell(tmp, "t4") as sh:
        a, b = two_probes(sh)
        def stall():
            try:
                sh.fast(timeout=600).click(a, "stall")
            except ConnectionError:
                pass  # never answers; the shell's exit drops it
        background(stall)
        time.sleep(0.2)
        fast = sh.fast()
        fast.dump_tree(b)
        time.sleep(1.5)
        tabs = {t["item_id"]: t for t in fast.request("ListTabs")}
        assert tabs[a]["responding"] is False, tabs
        # Refused at once, not queued behind the hang.
        assert timed(lambda: expect_error(fast, a, "not responding")) < RESPONSIVE
        fast.request("CloseItem", item_id=a)
        assert a not in {t["item_id"] for t in fast.request("ListTabs")}
        fast.dump_tree(b)
        assert sh.alive()


def spawn_node(tmp: Path) -> tuple[subprocess.Popen, str]:
    sock = tmp / "kunki.sock"
    env = {
        **os.environ,
        "OSVAULD_KUNKI_DIR": str(tmp / "node"),
        "OSVAULD_KUNKI_PASSPHRASE": "test",
        "OSVAULD_KUNKI_SOCKET": str(sock),
    }
    proc = subprocess.Popen([str(KUNKI)], env=env, stdout=subprocess.PIPE, text=True)
    ticket = proc.stdout.readline().strip()  # kunki's one stdout line; doubles as "booted"
    if not ticket:
        raise RuntimeError("kunki exited before printing a ticket")
    os.environ["OSVAULD_KUNKI_SOCKET"] = str(sock)
    return proc, ticket


@test
def t5_background_push(tmp):
    node, ticket = spawn_node(tmp)
    try:
        with Shell(tmp, "alice") as al, Shell(tmp, "bob") as bo:
            al.rpc.signup("alice", "test")
            al.rpc.claim_node(ticket)
            ws = al.rpc.create_workspace("threads")
            item = al.rpc.create_item(ws["id"], "probe", "app")
            al.rpc.write_file(item["id"], "main.lua", PROBE)
            al.rpc.open_item(item["id"])
            al.rpc.push_src(item["id"])
            bo.rpc.signup("bob", "test")
            bo.rpc.claim_node(al.rpc.invite())
            al.rpc.publish_all()
            bo.rpc.join_item(ws, item)
            bo.rpc.open_item(item["id"])
            bo.rpc.frame(2)
            probe_item(bo.rpc, ws["id"], "elsewhere")  # bob's focus moves off the probe
            bo.rpc.frame(2)
            seen = len(bo.views())

            al.rpc.click(item["id"], "add")

            def with_note():  # "elsewhere" is a probe too, but always sees 0 notes
                return [l for l in bo.views()[seen:] if l.split()[-1] != "0"]

            deadline = time.monotonic() + 5.0
            while not with_note() and time.monotonic() < deadline:
                time.sleep(0.1)
            if not with_note():
                # Data first, so a failure here means "Lua didn't run", not "sync didn't
                # deliver". AppDataGet reads the docs without running the app.
                assert "note:" in str(bo.rpc.read_data(item["id"])), "the push never reached bob"
                raise AssertionError("bob has the note, but his background probe never ran its view")
            # Once per push, not on every tick: the signals work keeps this green as it makes
            # the run skip what the push did not touch.
            time.sleep(1.0)
            assert len(with_note()) == 1, with_note()
    finally:
        node.terminate()


@test
def t6_idle_is_quiet(tmp):
    with Shell(tmp, "t6") as sh:
        two_probes(sh)
        seen = len(sh.views())
        assert seen, "no view lines on stdout; the test cannot see apps run"
        time.sleep(5)
        assert len(sh.views()) == seen, f"{len(sh.views()) - seen} views ran with nothing happening"


@test
def t7_parallel_tiles(tmp):
    with Shell(tmp, "t7") as sh:
        a, b = two_probes(sh)
        sh.rpc.click(a, "heavy")
        sh.rpc.click(b, "heavy")
        sh.rpc.open_item(a)

        def median_frame():
            return sorted(timed(lambda: sh.rpc.frame(1)) for _ in range(5))[2]

        single = median_frame()
        assert single > 0.04, f"one heavy tile framed in {single * 1000:.0f} ms: the view did not run"
        sh.rpc.request("SplitWith", item_id=b)
        split = median_frame()
        assert split - single < 0.02, f"one tile {single * 1000:.0f} ms, two {split * 1000:.0f} ms"

        # Both on screen and both touchable: a click lands in the tile it is over, and only there.
        adds = sorted((r for r in sh.rpc.rects() if r["id"] == "add"), key=lambda r: r["x"])
        assert len(adds) == 2, adds
        sh.rpc.click_at(adds[1]["x"] + 5, adds[1]["y"] + 5)
        sh.rpc.frame(2)
        assert sh.rpc.read_data(b)["probe"] and not sh.rpc.read_data(a)["probe"]


@test
def t8_reload_over_boundary(tmp):
    with Shell(tmp, "t8") as sh:
        a, _ = two_probes(sh)
        src = sh.rpc.read_file_versioned(a, "main.lua")
        sh.rpc.edit_file(a, "main.lua", src["revision"], [{"old_text": '"PROBE"', "new_text": '"PROBE2"'}])
        sh.rpc.reload_item(a)
        assert "PROBE2" in str(sh.rpc.dump_tree(a))
        src = sh.rpc.read_file_versioned(a, "main.lua")
        try:
            sh.rpc.edit_file(a, "main.lua", src["revision"], [{"old_text": "return function()", "new_text": "return function("}])
            sh.rpc.reload_item(a)
        except BridgeError:
            pass  # refused up front is as good as refused at reload
        assert "PROBE2" in str(sh.rpc.dump_tree(a)), "a bad edit replaced the running app"


@test
def t9_search(tmp):
    print("    T9 is smoke_search.py unchanged; run it there")


@test
def t10_own_window(tmp):
    with Shell(tmp, "t10") as sh:
        a, b = two_probes(sh)
        sh.rpc.open_item(a)
        ids = lambda: [r["id"] for r in sh.rpc.rects()]
        assert "add" in ids()
        sh.rpc.request("PopOut", item_id=a)
        tabs = {t["item_id"]: t for t in sh.rpc.request("ListTabs")}
        assert tabs[a]["window"] and tabs[a]["shown"] and not tabs[b]["window"], tabs
        # A left the main window: its focused tab shows a placeholder, not its tile.
        sh.rpc.frame(1)
        assert "add" not in ids(), ids()
        # Tile vs window is invisible to the app: it answers and acts as before.
        assert "PROBE" in str(sh.rpc.dump_tree(a))
        sh.rpc.click(a, "add")
        assert "note:" in str(sh.rpc.read_data(a))
        sh.rpc.request("DockIn", item_id=a)
        tabs = {t["item_id"]: t for t in sh.rpc.request("ListTabs")}
        assert not tabs[a]["window"] and tabs[a]["focused"], tabs
        sh.rpc.frame(1)
        assert "add" in ids(), ids()
        assert "PROBE" in str(sh.rpc.dump_tree(a))


@test
def t11_chrome_budget(tmp):
    print("    T11 needs the Lua chrome (lua-shell branch); deferred until it lands")


def main() -> int:
    only = [a.upper() for a in sys.argv[1:]] or list(TESTS)
    build_shell()
    if "T5" in only and subprocess.run(["cargo", "build", "-p", "kunki"], cwd=ROOT).returncode:
        raise SystemExit("kunki build failed")
    failed = []
    for name in only:
        knobs = ("OSVAULD_APP_BUDGET_MS", "OSVAULD_APP_MEMORY_MB", "OSVAULD_WATCHDOG_MS", "OSVAULD_KUNKI_SOCKET")
        saved = {k: os.environ.pop(k, None) for k in knobs}
        tmp = Path(tempfile.mkdtemp(prefix=f"osvauld-{name}-"))
        print(f"── {name}", flush=True)
        try:
            TESTS[name](tmp)
            print(f"   PASS {name}")
        except Exception as e:
            failed.append(name)
            print(f"   FAIL {name}: {type(e).__name__}: {e}")
            if os.environ.get("VERBOSE"):
                traceback.print_exc()
        finally:
            for k, v in saved.items():
                os.environ.pop(k, None)
                if v is not None:
                    os.environ[k] = v
    print(f"\n{len(only) - len(failed)}/{len(only)} passed; failing: {' '.join(failed) or 'none'}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
