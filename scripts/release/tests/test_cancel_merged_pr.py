"""Exercise the cleanup workflow's actual script without GitHub credentials."""

import json
import os
from pathlib import Path
import subprocess
import textwrap
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = (ROOT / ".github/workflows/cancel-merged-pr.yml").read_text()
SCRIPT = textwrap.dedent(WORKFLOW.split("        run: |\n", 1)[1])
BASE = "repos/sheprd-labs/sheprd/actions/runs"


def run(run_id, number=42, event="pull_request", status="in_progress"):
    return {"id": run_id, "event": event, "status": status,
            "pull_requests": [] if number is None else [{"number": number}]}


class CancelMergedPr(unittest.TestCase):
    def execute(self, pages=None, failure_status=None):
        cancelled = []

        def api(command, *, text):
            self.assertTrue(text)
            self.assertEqual(command[:2], ["gh", "api"])
            path = command[2]
            if "?" in path:
                self.assertTrue(path.startswith(f"{BASE}?event=pull_request&status="))
                self.assertIn("&per_page=100", path)
                self.assertEqual(command[3:], ["--paginate", "--slurp"])
                status = path.split("status=", 1)[1].split("&", 1)[0]
                return json.dumps((pages or {}).get(status, [{"workflow_runs": []}]))
            if path.endswith("/cancel"):
                self.assertEqual(command[3:], ["--method", "POST"])
                cancelled.append(int(path.split("/")[-2]))
                if failure_status:
                    raise subprocess.CalledProcessError(1, command)
                return ""
            self.assertEqual(path, f"{BASE}/1")
            return json.dumps({"status": failure_status})

        with mock.patch.dict(os.environ, GITHUB_REPOSITORY="sheprd-labs/sheprd", PR_NUMBER="42"), \
                mock.patch("subprocess.check_output", side_effect=api), \
                mock.patch("builtins.print"):
            exec(compile(SCRIPT, "cancel-merged-pr.yml", "exec"), {})
        return cancelled

    def test_cancels_only_this_pr_across_pages_and_active_states(self):
        pages = {
            "in_progress": [
                {"workflow_runs": [run(1), run(2, number=99), run(3, event="push"),
                                   run(4, number=None), run(5, status="completed")]},
                {"workflow_runs": [run(6)]},
            ],
        }
        for run_id, status in enumerate(("queued", "waiting", "pending", "requested"), 7):
            pages[status] = [{"workflow_runs": [run(run_id, status=status), run(1)]}]
        self.assertEqual(self.execute(pages), [1, 6, 7, 8, 9, 10])

    def test_no_matching_runs_is_successful(self):
        self.assertEqual(self.execute(), [])

    def test_completion_race_is_harmless(self):
        pages = {"in_progress": [{"workflow_runs": [run(1)]}]}
        self.assertEqual(self.execute(pages, failure_status="completed"), [1])

    def test_failed_cancellation_of_active_run_is_reported(self):
        pages = {"in_progress": [{"workflow_runs": [run(1)]}]}
        with self.assertRaises(subprocess.CalledProcessError):
            self.execute(pages, failure_status="in_progress")

    def test_cleanup_uses_base_workflow_only_after_merge(self):
        self.assertIn("  pull_request_target:", WORKFLOW)
        self.assertIn("    types: [closed]", WORKFLOW)
        self.assertIn("github.event.pull_request.merged == true", WORKFLOW)
        self.assertIn("github.repository == 'sheprd-labs/sheprd'", WORKFLOW)
        self.assertIn("permissions: {}", WORKFLOW)
        self.assertIn("      actions: write", WORKFLOW)
        self.assertNotIn("uses:", WORKFLOW)
        self.assertNotIn("${{", SCRIPT)


if __name__ == "__main__":
    unittest.main()
