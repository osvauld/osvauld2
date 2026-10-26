"""Frame summaries must not mix host and app threads."""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class FrameTimingSummaryTests(unittest.TestCase):
    def test_threads_are_separate_and_captures_are_excluded(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "timings.log"
            log.write_text(
                "OSVAULD_FRAME total_ms=10.0 capture=false thread=ThreadId(1)\n"
                "OSVAULD_FRAME total_ms=7.0 capture=false thread=ThreadId(2)\n"
                "OSVAULD_FRAME total_ms=99.0 capture=true thread=ThreadId(1)\n"
            )
            output = subprocess.check_output(
                [sys.executable, str(Path(__file__).with_name("summarize_frame_timings.py")), str(log)],
                text=True,
            )
            self.assertIn("ThreadId(1): 1 normal frames", output)
            self.assertIn("ThreadId(2): 1 normal frames", output)
            self.assertNotIn("99.000", output)


if __name__ == "__main__":
    unittest.main()
