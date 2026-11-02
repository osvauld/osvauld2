"""The laptop-host smoke: H1 (an account hosts for itself), H2 (join by link, items by name,
the host's data arrives) and H3 (live both ways) from docs/design/laptop-host.md §0, over two
offscreen shells and iroh on loopback. The full set is `scripts/e2e_host.py`.

    python3 scripts/smoke_host.py
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import e2e_host  # noqa: E402


def main() -> int:
    sys.argv = [sys.argv[0], "h1", "h2", "h3"]
    return e2e_host.main()


if __name__ == "__main__":
    sys.exit(main())
