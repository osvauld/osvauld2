"""Run a visual smoke command on a worker with Xvfb and no desktop session.

    python3 scripts/with_xvfb.py python3 scripts/smoke.py

Xvfb chooses its display through -displayfd. No fixed display number or private lock protocol.
"""
import os
import select
import subprocess
import sys
import tempfile


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    read, write = os.pipe()
    server = subprocess.Popen(
        ["Xvfb", "-displayfd", str(write), "-screen", "0", "1280x1024x24", "-nolisten", "tcp"],
        pass_fds=(write,), stdout=subprocess.DEVNULL,
    )
    os.close(write)
    try:
        if not select.select([read], [], [], 15)[0]:
            raise RuntimeError("Xvfb did not publish a display within 15 seconds")
        display = os.read(read, 64).decode().strip()
        if not display.isdecimal():
            raise RuntimeError(f"Xvfb did not start: {display!r}")
        with tempfile.TemporaryDirectory(prefix="osvauld-display-") as runtime:
            env = dict(os.environ, DISPLAY=f":{display}", XDG_RUNTIME_DIR=runtime)
            env.pop("WAYLAND_DISPLAY", None)
            env.pop("WAYLAND_SOCKET", None)
            return subprocess.run(sys.argv[1:], env=env).returncode
    finally:
        os.close(read)
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
            server.wait()


if __name__ == "__main__":
    raise SystemExit(main())
