"""Session must use the selected binary, not a stale worktree-local build."""
import os
import unittest
from pathlib import Path
from unittest.mock import patch

from .session import DEFAULT_SHELL_BINARY, Session


class SessionBinaryTests(unittest.TestCase):
    def session(self, **kwargs):
        session = Session(**kwargs)
        self.addCleanup(session.close)
        return session

    def test_environment_selects_the_default_binary(self):
        with patch.dict(os.environ, {"OSVAULD_SHELL_BINARY": "/tmp/selected-shell"}):
            self.assertEqual(self.session().shell_binary, Path("/tmp/selected-shell"))

    def test_explicit_binary_takes_precedence(self):
        with patch.dict(os.environ, {"OSVAULD_SHELL_BINARY": "/tmp/selected-shell"}):
            self.assertEqual(self.session(shell_binary=Path("/tmp/explicit-shell")).shell_binary,
                             Path("/tmp/explicit-shell"))

    def test_empty_override_uses_the_worktree_default(self):
        with patch.dict(os.environ, {"OSVAULD_SHELL_BINARY": ""}):
            self.assertEqual(self.session().shell_binary, DEFAULT_SHELL_BINARY)
