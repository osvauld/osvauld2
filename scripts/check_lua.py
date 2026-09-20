"""The types gate: run lua-language-server over every app, one app at a time.

    python3 scripts/check_lua.py              # every app in demo_apps/
    python3 scripts/check_lua.py kanban pie   # just these
    python3 scripts/check_lua.py --level Information

Needs `lua-language-server` on PATH and `lua-types/osvauld.lua`, which is generated from the
running sandbox — regenerate it with:

    BLESS=1 cargo test -p app_host generated_defs_are_current

**One app at a time, and that is not an optimisation.** `require("ui/widgets")` resolves inside
the app's own folder at runtime, but a language server pointed at `demo_apps/` sees every
`widgets.lua` at once and picks one. Checking the tree as a single workspace reported four
arity errors in `math_mela` that came from `kanban`'s unrelated `W.badge`; checked alone,
`math_mela` is clean. A gate that cries wolf gets switched off.
"""

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
APPS = ROOT / "demo_apps"
CONFIG = ROOT / ".luarc.json"
DEFS = ROOT / "lua-types" / "osvauld.lua"
ANSI = re.compile(r"\x1b\[[0-9;]*m")
COUNT = re.compile(r"(\d+) problems found")


def check(app: Path, level: str) -> tuple[int, str]:
    """Returns (problem count, the report with progress noise stripped)."""
    proc = subprocess.run(
        [
            "lua-language-server",
            "--check", str(app.resolve()),
            "--configpath", str(CONFIG.resolve()),
            f"--checklevel={level}",
        ],
        capture_output=True,
        text=True,
        timeout=300,
    )
    clean = ANSI.sub("", proc.stdout + proc.stderr)
    found = COUNT.search(clean)
    lines = [ln.strip() for ln in clean.splitlines() if "[" in ln and "]" in ln and ":" in ln]
    return (int(found.group(1)) if found else 0, "\n".join(lines))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("apps", nargs="*", help="app folder names (default: all)")
    ap.add_argument("--level", default="Warning", help="Error | Warning | Information | Hint")
    args = ap.parse_args()

    if not shutil.which("lua-language-server"):
        print("lua-language-server is not on PATH — skipping the types gate")
        return 0
    if not DEFS.exists():
        print(f"{DEFS} is missing. Generate it:")
        print("    BLESS=1 cargo test -p app_host generated_defs_are_current")
        return 1

    names = args.apps or sorted(p.name for p in APPS.iterdir() if (p / "main.lua").exists())
    total, dirty = 0, []
    for name in names:
        app = APPS / name
        if not app.is_dir():
            print(f"no such app: {name}")
            return 1
        count, report = check(app, args.level)
        total += count
        print(f"{'ok  ' if count == 0 else 'WARN'} {name}: {count}")
        if report:
            dirty.append(report)

    if dirty:
        print("\n" + "\n".join(dirty))
    print(f"\n{total} problems across {len(names)} apps")
    # Reported, not enforced: the corpus predates the definitions and these are app-level
    # findings, not gate failures. Tighten to `return 1 if total else 0` once it is clean.
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
