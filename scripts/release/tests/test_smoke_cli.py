"""Release CLI smoke check against a mock executable; no Cargo or real app."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "smoke-cli.py"
SPEC = importlib.util.spec_from_file_location("smoke_cli", SCRIPT)
SMOKE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SMOKE)

# Answers like a release build unless MOCK_BREAK names a behavior to break.
MOCK = r'''
import os, sys, time
args, broken = sys.argv[1:], os.environ.get("MOCK_BREAK", "")
assert "HERDR_PANE_ID" not in os.environ
usage = "Usage: sheprd [OPTIONS]\n"
if broken == "hang":
    time.sleep(60)
if args in (["--help"], ["-h"]):
    options = ["--socket CLIENT_SOCKET", "--session NAME", "--dev", "--build-info"]
    if broken == "help":
        options.pop()
    if broken == "feature":
        options.append("--integration-test")
    print(usage + "\n".join(options))
elif args == ["--build-info"]:
    print("worktree=\nbranch=main\npr=" + ("\nextra=1" if broken == "build-info" else ""))
elif args == ["--socket"]:
    sys.stderr.write("--socket requires a path\n" + usage)
    sys.exit(2)
elif args == ["browser", "open"]:
    sys.stderr.write("browser open requires one URL\n" + usage)
    sys.exit(2)
else:
    sys.stderr.write(f"Unknown option: {args[0]}\n" + usage)
    sys.exit(134 if broken == "abort" else 2)
'''


@unittest.skipIf(sys.platform == "win32", "the mock executable is a shebang script")
class SmokeCliTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.executable = Path(temp.name) / "sheprd"
        self.executable.write_text(f"#!{sys.executable}\n{MOCK}")
        self.executable.chmod(0o755)
        self.environ = dict(os.environ, HERDR_PANE_ID="1")

    def smoke(self, broken=""):
        with mock.patch.dict(os.environ, dict(self.environ, MOCK_BREAK=broken), clear=True):
            SMOKE.smoke(self.executable)

    def test_a_release_build_passes(self):
        self.smoke()

    def test_each_broken_behavior_fails(self):
        for broken in ("help", "feature", "build-info", "abort"):
            with self.subTest(broken=broken), self.assertRaises(SMOKE.SmokeError):
                self.smoke(broken)

    def test_a_hung_executable_times_out(self):
        with mock.patch.object(SMOKE, "TIMEOUT", 1), self.assertRaisesRegex(SMOKE.SmokeError, "did not exit"):
            self.smoke("hang")

    def test_a_missing_executable_is_a_usage_error(self):
        with self.assertRaises(SystemExit) as raised, contextlib.redirect_stderr(io.StringIO()):
            SMOKE.main([str(self.executable.with_name("missing"))])
        self.assertEqual(raised.exception.code, 2)
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(SMOKE.main([str(self.executable)]), 0)


if __name__ == "__main__":
    unittest.main()
