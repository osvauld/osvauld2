"""Manual check (design/hidden-window.md): the shell answers while its window is hidden.

Open an animating app (e.g. the jetpack pilot editor, preview playing), hide the shell window
(another workspace, or minimise), then run:

    python3 scripts/check_hidden_window.py /tmp/osvauld.sock ITEM_ID

Each request must answer in under 2 s. Needs a real compositor, so it is not in SMOKES."""
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.client import Bridge

socket, item = sys.argv[1], sys.argv[2]
r = Bridge(socket, timeout=35)
slow = []
for name, call in [
    ("ping", lambda: r.ping()),
    ("read_data", lambda: r.read_data(item)),
    ("read_console", lambda: r.read_console(item)),
    ("screenshot", lambda: r.screenshot(item)),
    ("ping again", lambda: r.ping()),
]:
    start = time.monotonic()
    call()
    took = time.monotonic() - start
    print(f"{name}: {took:.2f}s")
    if took > 2:
        slow.append(name)
assert not slow, f"slow while hidden: {slow}"
print("hidden window: the shell kept answering")
