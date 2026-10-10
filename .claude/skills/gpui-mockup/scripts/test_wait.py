"""Isolated receiver tests: no running app or personal control socket is used."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


class WaitTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="mockup-wait-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.script = self.root / ".claude/skills/gpui-mockup/scripts/wait.sh"
        self.script.parent.mkdir(parents=True)
        shutil.copyfile(Path(__file__).with_name("wait.sh"), self.script)
        app = self.root / "target/debug/herdr-gpui"
        app.parent.mkdir(parents=True)
        app.write_text(
            f"#!{sys.executable}\n"
            "import json, os, pathlib, signal, sys\n"
            "root = pathlib.Path(os.environ['TEST_ROOT'])\n"
            "(root / 'called').write_text(json.dumps(sys.argv[1:]))\n"
            "mode = os.environ['TEST_MODE']\n"
            "if mode == 'notes':\n"
            "    sys.stdout.write('Picked: B\\n')\n"
            "    sys.exit(0)\n"
            "if mode == 'waiting':\n"
            "    (root / 'pid').write_text(str(os.getpid()))\n"
            "    signal.pause()\n"
            "if mode == 'refused':\n"
            "    sys.stderr.write('Request refused\\n')\n"
            "    sys.exit(1)\n"
            "sys.exit(3 if mode == 'absent' else 4)\n"
        )
        app.chmod(0o700)
        self.feedback = self.root / "feedback.md"
        self.env = dict(os.environ, HERDR_PANE_ID="test:pane", TEST_ROOT=str(self.root))

    def run_wait(self, seconds=0, mode="empty"):
        return subprocess.run(
            ["bash", str(self.script), str(self.feedback), str(seconds)],
            env=dict(self.env, TEST_MODE=mode),
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )

    def test_zero_checks_socket_without_wait(self):
        result = self.run_wait(mode="notes")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "Picked: B\n")
        self.assertEqual(json.loads((self.root / "called").read_text()), ["browser", "feedback"])

    def test_zero_consumes_file_when_socket_has_no_notes_or_app_is_absent(self):
        for index, mode in enumerate(["empty", "absent"], 1):
            with self.subTest(mode=mode):
                self.feedback.write_text(f"Notes {index}\n")
                result = self.run_wait(mode=mode)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, f"Notes {index}\n")
                self.assertFalse(self.feedback.exists())
                self.assertEqual(Path(f"{self.feedback}.{index}").read_text(), result.stdout)

    def test_zero_with_no_feedback_exits_four(self):
        for mode in ["empty", "absent"]:
            with self.subTest(mode=mode):
                result = self.run_wait(mode=mode)
                self.assertEqual(result.returncode, 4, result.stderr)
                self.assertEqual(result.stdout, "")

    def test_refusal_is_reported_without_hiding_file_feedback(self):
        self.feedback.write_text("Legacy app fallback\n")
        result = self.run_wait(mode="refused")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "Legacy app fallback\n")
        self.assertEqual(result.stderr, "Request refused\n")

    def test_file_only_wait_consumes_once(self):
        self.env.pop("HERDR_PANE_ID")
        self.feedback.write_text("File notes\n")
        self.assertEqual(self.run_wait().stdout, "File notes\n")
        self.assertEqual(self.run_wait().returncode, 4)
        self.assertFalse((self.root / "called").exists())

    def test_older_app_wait_does_not_hide_fallback_send(self):
        # Older apps accept browser.feedback but reject notes.send, so the
        # mockup writes its fallback while the feedback request is still open.
        process = subprocess.Popen(
            ["bash", str(self.script), str(self.feedback), "60"],
            env=dict(self.env, TEST_MODE="waiting"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            deadline = time.monotonic() + 5
            pid_file = self.root / "pid"
            while not pid_file.exists():
                if process.poll() is not None or time.monotonic() >= deadline:
                    self.fail("Feedback waiter did not start")
                time.sleep(0.01)
            # The marker is written before publishing the fallback, so this
            # cannot pass by checking a file that predates the socket wait.
            pending = self.root / "pending.md"
            pending.write_text("Fallback notes\n")
            pending.replace(self.feedback)
            stdout, stderr = process.communicate(timeout=5)
            self.assertEqual(process.returncode, 0, stderr)
            self.assertEqual(stdout, "Fallback notes\n")
            self.assertEqual(json.loads((self.root / "called").read_text()),
                             ["browser", "feedback", "--wait", "60"])
            self.assertEqual(Path(f"{self.feedback}.1").read_text(), stdout)
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid_file.read_text()), 0)
        finally:
            if process.poll() is None:
                process.terminate()
            process.communicate(timeout=5)


if __name__ == "__main__":
    unittest.main()
