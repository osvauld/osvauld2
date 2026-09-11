"""upload_app — push an app folder over the bridge, open it, keep the shell alive.

    python3 scripts/upload_app.py                          # kanban, fresh throwaway store
    python3 scripts/upload_app.py --keep                   # reuse ./osvauld-drive-data
    python3 scripts/upload_app.py --keep demo_apps/tally   # any app folder

Logs in (or signs up on a fresh store), creates a workspace + app item, uploads every
.lua/.osv file the folder holds (the GUI picker's walk, one WriteFile per file), and
opens the item's tab. The window stays up until Enter is pressed — Ctrl-D in the REPL
world, Enter here — then the spawned shell is torn down.
"""

import argparse
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session

DEFAULT_APP = Path(__file__).parent.parent / "shell2/src/kanban"


def main() -> None:
    ap = argparse.ArgumentParser(description="upload an app folder over the bridge and open it")
    ap.add_argument("folder", nargs="?", default=DEFAULT_APP, help="app folder (root main.lua)")
    ap.add_argument("--keep", action="store_true",
                    help="reuse ./osvauld-drive-data (unlocks 'abe' / 'correct horse')")
    ap.add_argument("--name", help="item name (default: folder name)")
    args = ap.parse_args()

    folder = Path(args.folder).resolve()
    if not folder.is_dir():
        sys.exit(f"not a folder: {folder}")

    data_dir = os.path.abspath("osvauld-drive-data") if args.keep else None
    with Session(data_dir=data_dir) as s:
        accounts = s.rpc.list_accounts()
        if accounts:
            print("unlock:", s.rpc.unlock(accounts[0]["name"], "correct horse" if args.keep else "test"))
        else:
            print("signup: mnemonic shown once —", s.rpc.signup("abe", "test")["mnemonic"][:24] + "…")
        ws = s.rpc.create_workspace("apps")
        item = s.rpc.create_item(ws["id"], args.name or folder.name, "app")
        files = s.rpc.upload_folder(item["id"], folder)
        print(f"uploaded {len(files)} file(s): {', '.join(files)}")
        print("open:", s.rpc.open_item(item["id"]))
        input(f"\n'{item['name']}' is running — press Enter to tear the shell down: ")


if __name__ == "__main__":
    main()
