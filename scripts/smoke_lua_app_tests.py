"""Run app-shipped Lua tests for small demo apps over the real bridge."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session

ROOT = Path(__file__).parent.parent
APPS = ["tally", "scratch", "pomodoro"]

with Session() as s:
    s.rpc.signup("abe", "test")
    ws = s.rpc.create_workspace("lua app tests")
    for name in APPS:
        item = s.rpc.create_item(ws["id"], name, "app")
        uploaded = s.rpc.upload_folder(item["id"], ROOT / "demo_apps" / name)
        assert any(p.startswith("tests/") for p in uploaded), f"{name}: no tests uploaded"
        results = s.rpc.run_tests(item["id"])
        failed = [r for r in results if not r["ok"]]
        assert results, f"{name}: no test results"
        assert not failed, f"{name}: {failed}"
        print(f"{name}: {len(results)} lua test(s) ok")

print("smoke ok: app-shipped Lua tests for small demos")
