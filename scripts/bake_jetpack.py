"""Bake Jetpack's workspace assets into the game's source (design/jetpack-editors.md).

Reads the assets an editor holds open, over the bridge, and writes them as data-only Lua under
each app's `baked/`, which is what ships: desktop-to-phone asset sync is not built.

    python3 scripts/bake_jetpack.py --item <map editor id> --item <pilot editor id> --item <gun editor id> [--socket PATH] [--root DIR]
"""
import argparse
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.client import Bridge

ROOT = Path(__file__).resolve().parent.parent
# asset name → (baked file, the apps that ship it)
ASSETS = {
    "jetpack-maps": ("baked/maps.lua", ("jetpack", "jetpack_map_editor")),
    "jetpack-pilots": ("baked/pilots.lua", ("jetpack", "jetpack_pilot_editor", "jetpack_gun_editor")),
    "jetpack-guns": ("baked/guns.lua", ("jetpack", "jetpack_gun_editor", "jetpack_pilot_editor", "jetpack_map_editor")),
}
VERSIONS = {"jetpack-maps": 1, "jetpack-pilots": 2, "jetpack-guns": 1}  # the info.version each kind is baked at
FIRST = ("id", "name", "kind", "team", "x", "y", "w", "h")
IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def lua(value, depth=0):
    """Data-only Lua with a stable key order, so a re-bake of the same asset is no diff."""
    pad, inner = "\t" * depth, "\t" * (depth + 1)
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        if value != value or value in (float("inf"), float("-inf")):
            raise ValueError("not a finite number")
        return str(int(value)) if float(value).is_integer() else repr(float(value))
    if isinstance(value, str):
        return '"' + value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'
    if isinstance(value, list):
        if not value:
            return "{}"
        flat = all(not isinstance(v, (list, dict)) for v in value)
        if flat:
            return "{ " + ", ".join(lua(v) for v in value) + " }"
        return "{\n" + "".join(f"{inner}{lua(v, depth + 1)},\n" for v in value) + pad + "}"
    if isinstance(value, dict):
        keys = [k for k in FIRST if k in value] + sorted(k for k in value if k not in FIRST)
        def key(k):
            return k if IDENT.match(k) else "[" + lua(k) + "]"
        flat = all(not isinstance(value[k], (list, dict)) for k in keys)
        if flat:
            return "{ " + ", ".join(f"{key(k)} = {lua(value[k])}" for k in keys) + " }"
        return "{\n" + "".join(f"{inner}{key(k)} = {lua(value[k], depth + 1)},\n" for k in keys) + pad + "}"
    raise ValueError(f"cannot bake {type(value).__name__}")


def source(name, data):
    return f"-- Baked by scripts/bake_jetpack.py from the workspace asset `{name}`. Data only.\nreturn {lua(data)}\n"


def bake(rpc, items, root=ROOT):
    """Writes every asset the items hold; returns the files written."""
    held = {}
    for item in [items] if isinstance(items, str) else items:
        held.update(rpc.read_data(item))
    written = []
    for name, (file, apps) in ASSETS.items():
        data = held.get(f"asset:{name}")
        if not data or data.get("info", {}).get("version") != VERSIONS[name]:
            continue
        text = source(name, data)
        for app in apps:
            path = Path(root) / "demo_apps" / app / file
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
            written.append(path)
    return written


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--item", required=True, action="append", help="an open editor holding assets (repeat)")
    ap.add_argument("--socket", default=os.environ.get("OSVAULD_SOCKET", "/tmp/osvauld.sock"))
    ap.add_argument("--root", default=str(ROOT), help="repository root to write into")
    args = ap.parse_args()
    written = bake(Bridge(args.socket), args.item, args.root)
    if not written:
        sys.exit("nothing baked: no item holds a jetpack asset (open it in the shell first)")
    for path in written:
        print(f"baked {path}")


if __name__ == "__main__":
    main()
