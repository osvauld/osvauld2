"""Every smoke, one command, no window.

    python3 scripts/smoke.py

There is no CI in this repo and the smokes are scripts, so the failure mode they have is not
failing — it is never being run. This is the one command to run before landing anything that
touches the shell, the bridge or the runtime, and it is the command to add a new smoke to.

Windowless by default: `OSVAULD_OFFSCREEN` makes every `Session` spawn `shell2 --offscreen`,
including in scripts that were written before offscreen mode existed and know nothing about it.
Pass `--windowed` to watch it happen instead.

`cargo test` covers the runtime internals (`Headless`); these cover the path an agent actually
drives — a real socket, real pixels, a real Lua VM. Neither substitutes for the other.
"""

import argparse
import os
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).parent

# Order matters: the cheapest and most fundamental first, so a broken socket reports as a broken
# socket rather than as a confusing failure three minutes into a rendering test.
SMOKES = ["smoke_bridge.py", "smoke_offscreen.py", "smoke_tank_keyboard.py", "smoke_tank_game.py"]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--windowed", action="store_true", help="open a real window (to watch)")
    ap.add_argument("--size", default="900x700", help="offscreen viewport, WxH")
    ap.add_argument("only", nargs="*", help="run only these smokes")
    args = ap.parse_args()

    env = dict(os.environ)
    if args.windowed:
        env.pop("OSVAULD_OFFSCREEN", None)
    else:
        env["OSVAULD_OFFSCREEN"] = args.size

    # Build once here rather than letting each smoke rebuild: they all call `build_shell`, and a
    # second cargo invocation just blocks on the package lock.
    sys.path.insert(0, str(HERE))
    from osvauld.session import build_shell

    build_shell()
    env["OSVAULD_SHELL_BINARY"] = env.get("OSVAULD_SHELL_BINARY") or str(
        HERE.parent / "target" / "debug" / "shell2"
    )

    failed = []
    for name in args.only or SMOKES:
        print(f"\n── {name} " + "─" * (60 - len(name)), flush=True)
        started = time.monotonic()
        code = subprocess.run([sys.executable, str(HERE / name)], env=env).returncode
        took = time.monotonic() - started
        print(f"   {'ok' if code == 0 else 'FAILED'} in {took:.1f}s", flush=True)
        if code != 0:
            failed.append(name)

    print()
    if failed:
        print(f"FAILED: {', '.join(failed)}")
        return 1
    print(f"all smokes ok ({len(args.only or SMOKES)})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
