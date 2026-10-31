"""Bundled apps (docs/design/ios.md I4): apps shipped inside the binary's bundle are installed at signup.

`OSVAULD_BUNDLED_APPS` names a folder of app folders; on iOS it defaults to `apps/` inside the `.app`.
A new account gets one "Apps" space holding each of them, ready to open. Unlock never installs
again, and without the folder signup installs nothing.
"""
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
JETPACK = "Skyward · Jetpack Arena"


def spaces(r, name):
    return [w for w in r.list_workspaces() if w["name"] == name]


def with_bundle():
    with tempfile.TemporaryDirectory() as bundle:
        (Path(bundle) / "jetpack").symlink_to(ROOT / "demo_apps/jetpack")
        os.environ["OSVAULD_BUNDLED_APPS"] = bundle
        try:
            with Session(shell_binary=shell_binary()) as s:
                r = s.rpc
                r.signup("pilot", "bundled apps smoke passphrase")
                apps = spaces(r, "Apps")
                assert len(apps) == 1, f"one Apps space after signup, got {r.list_workspaces()}"
                items = r.list_items(apps[0]["id"])
                assert [(i["name"], i["kind"]) for i in items] == [(JETPACK, "app")], items
                item = items[0]["id"]
                assert "main.lua" in [f if isinstance(f, str) else f.get("path") for f in r.list_files(item)]
                r.open_item(item)
                r.frame(3)
                errors = [l for l in r.read_console(item) if "error" in l.lower()]
                assert not errors, errors
                assert r.dump_tree(item).get("children"), "Jetpack drew nothing"

                r.lock()
                r.unlock("pilot", "bundled apps smoke passphrase")
                assert len(spaces(r, "Apps")) == 1, "unlock must not install again"
                assert len(r.list_items(spaces(r, "Apps")[0]["id"])) == 1
        finally:
            del os.environ["OSVAULD_BUNDLED_APPS"]
    print("bundled apps: installed at signup, Jetpack runs, unlock adds nothing")


def without_bundle():
    with Session(shell_binary=shell_binary()) as s:
        r = s.rpc
        r.signup("plain", "bundled apps smoke passphrase")
        assert not spaces(r, "Apps"), r.list_workspaces()
    print("bundled apps: none without a bundle")


if __name__ == "__main__":
    os.environ.pop("OSVAULD_BUNDLED_APPS", None)
    with_bundle()
    without_bundle()
