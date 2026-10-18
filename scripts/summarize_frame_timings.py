"""Summarize elapsed frame stages per thread; NOT input-to-display/GPU timestamps."""
import math
import re
import sys
from collections import defaultdict
from pathlib import Path

groups = defaultdict(list)
for path in sys.argv[1:]:
    for line in Path(path).read_text().splitlines():
        if not line.startswith("OSVAULD_FRAME ") or "capture=true" in line:
            continue
        thread = re.search(r"thread=(\S+)", line)
        key = (path, thread[1] if thread else "legacy")
        groups[key].append({key: float(value) for key, value in re.findall(r"(\w+_ms)=([\d.]+)", line)})
if not groups:
    raise SystemExit("No normal frames found. Launch with OSVAULD_FRAME_TIMINGS=1 and redirect stderr to a log.")
for (path, thread), rows in sorted(groups.items()):
    print(f"{path} {thread}: {len(rows)} normal frames; elapsed stages (including waits; no display latency)")
    for key in rows[0]:
        values = sorted(row[key] for row in rows)
        def percentile(p):
            return values[max(0, math.ceil(len(values) * p) - 1)]
        print(f"{key:24} p50={percentile(.5):8.3f} p95={percentile(.95):8.3f} max={values[-1]:8.3f}")
