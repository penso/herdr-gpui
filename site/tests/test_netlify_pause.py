"""Netlify must skip automatic builds in every deployment context."""
from pathlib import Path
import subprocess
import tomllib
import unittest


class NetlifyPauseTests(unittest.TestCase):
    def test_automatic_builds_are_ignored_in_every_context(self):
        root = Path(__file__).resolve().parents[2]
        config = tomllib.loads((root / "netlify.toml").read_text())
        for context in ("production", "deploy-preview", "branch-deploy"):
            with self.subTest(context=context):
                settings = config["build"] | config.get("context", {}).get(context, {})
                result = subprocess.run(
                    ["bash", "-c", settings["ignore"]],
                    env={"CONTEXT": context},
                    cwd=root,
                    capture_output=True,
                    timeout=5,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
