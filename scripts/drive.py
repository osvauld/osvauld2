"""drive — the interactive bridge client: a live shell plus a Python REPL.

    python3 scripts/drive.py                          # fresh throwaway shell
    python3 scripts/drive.py --keep                   # reuse ./osvauld-drive-data across runs
    python3 scripts/drive.py --verbose                # show the shell's own logs
    python3 scripts/drive.py --socket /tmp/osvauld.sock   # attach to an already-running shell

The window stays open for as long as the REPL does (Ctrl-D or exit() to quit,
which tears the spawned shell down). `rpc` is a Bridge; today it speaks the
read-only family, auth, workspaces/items, and app source files — Ping,
ListAccounts, ListWorkspaces, Signup, Unlock, Lock, CreateWorkspace,
ListItems, CreateItem, ListFiles, ReadFile, WriteFile, ReloadItem,
OpenItem — with the senses landing slice by slice.
"""

import argparse
import code
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.client import Bridge
from osvauld.session import Session


def main() -> None:
    ap = argparse.ArgumentParser(description="drive an osvauld shell over its bridge")
    ap.add_argument("--socket", help="attach to a running shell at this socket path")
    ap.add_argument("--keep", action="store_true", help="reuse a persistent data dir")
    ap.add_argument("--verbose", action="store_true", help="show the shell's stdout/stderr")
    args = ap.parse_args()

    if args.socket:
        rpc = Bridge(args.socket)
        print(f"attached: {args.socket} — ping says {rpc.ping()!r}")
        code.interact(banner="rpc is live. Ctrl-D to detach.", local={"rpc": rpc})
        return

    data_dir = os.path.abspath("osvauld-drive-data") if args.keep else None
    session = Session(data_dir=data_dir, show_shell_output=args.verbose)
    session.start()
    try:
        print(f"shell up: data={session.data_dir} socket={session.socket_path}")
        print(f"ping says {session.rpc.ping()!r}; Ctrl-D tears it down")
        code.interact(banner="", local={"rpc": session.rpc, "s": session})
    finally:
        session.close()


if __name__ == "__main__":
    main()
