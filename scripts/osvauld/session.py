"""Session — spawn a shell2 instance with an isolated store and bridge socket.

    from osvauld.session import Session
    with Session() as s:
        accounts = s.rpc.list_accounts()

Modeled on the old repo's harness: throwaway OSVAULD_DATA_DIR, socket beside
it, wait for ping before returning, terminate on exit.
"""

import os
import shutil
import socket
import subprocess
import tempfile
import time
from pathlib import Path

from .client import Bridge

DEFAULT_SHELL_BINARY = Path(__file__).parent.parent.parent / "target" / "debug" / "shell2"


class Session:
    def __init__(
        self,
        shell_binary: Path = DEFAULT_SHELL_BINARY,
        startup_timeout: float = 15.0,
        data_dir: str | None = None,
        socket_path: str | None = None,
        show_shell_output: bool = False,
    ):
        self.tmp = tempfile.mkdtemp(prefix="osvauld-test-")
        self.data_dir = data_dir or os.path.join(self.tmp, "data")
        self.socket_path = socket_path or os.path.join(self.tmp, "bridge.sock")
        self.shell_binary = shell_binary
        self.startup_timeout = startup_timeout
        self.show_shell_output = show_shell_output
        self.process: subprocess.Popen | None = None
        self.rpc = Bridge(self.socket_path)

    def __enter__(self) -> "Session":
        self.start()
        return self

    def __exit__(self, *_exc) -> None:
        self.close()

    def start(self) -> None:
        env = {
            **os.environ,
            "OSVAULD_DATA_DIR": self.data_dir,
            "OSVAULD_SOCKET": self.socket_path,
        }
        self.process = subprocess.Popen(
            [str(self.shell_binary)], env=env,
            stdout=None if self.show_shell_output else subprocess.DEVNULL,
            stderr=None if self.show_shell_output else subprocess.DEVNULL,
        )
        deadline = time.monotonic() + self.startup_timeout
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"shell2 exited early with {self.process.returncode}")
            if os.path.exists(self.socket_path):
                try:
                    if self.rpc.ping() == "pong":
                        return
                except (ConnectionError, OSError):
                    pass  # binding races: retry until the deadline
            time.sleep(0.05)
        self.close()
        raise TimeoutError(f"bridge socket never answered: {self.socket_path}")

    def close(self) -> None:
        if self.process and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
        self.process = None
        shutil.rmtree(self.tmp, ignore_errors=True)
