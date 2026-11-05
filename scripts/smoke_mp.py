"""smoke_mp.py — multiplayer on the real path (docs/design/jetpack-mp.md §0): a channel message
across three shells (M1), a `net.match` converging (M4), and a two-player world match (M6).
The rest of the suite is scripts/e2e_mp.py."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import e2e_mp  # noqa: E402

if __name__ == "__main__":
    sys.argv = [sys.argv[0], "m1", "m4", "m6"]
    sys.exit(e2e_mp.main())
