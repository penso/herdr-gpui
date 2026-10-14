"""CI eligibility and credential boundaries; GitHub enforces fork approval online."""

import ast
import itertools
import os
from pathlib import Path
import re
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = (ROOT / ".github/workflows/ci.yml").read_text()
SECTIONS = re.split(r"^  ([a-z-]+):\n", WORKFLOW.split("\njobs:\n", 1)[1], flags=re.M)
JOBS = dict(zip(SECTIONS[1::2], SECTIONS[2::2]))
PR_JOBS = ("zizmor", "commits", "checks", "windows", "checks-passed")


def condition_matches(job, context):
    # Interpret only the boolean/equality subset used by these job conditions.
    # Unsupported syntax fails rather than executing workflow text as Python.
    expression = re.search(r"^    if: >-\n((?:      .+\n)+)", JOBS[job], re.M)[1]
    expression = " ".join(expression.split()).replace("&&", "and").replace("||", "or")

    def evaluate(node):
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            return node.value
        if isinstance(node, ast.Attribute):
            return context[ast.unparse(node)]
        if isinstance(node, ast.BoolOp):
            values = [evaluate(value) for value in node.values]
            if isinstance(node.op, ast.And):
                return all(values)
            if isinstance(node.op, ast.Or):
                return any(values)
        if isinstance(node, ast.Compare) and len(node.ops) == 1 and isinstance(node.ops[0], ast.Eq):
            return evaluate(node.left) == evaluate(node.comparators[0])
        if (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                and node.func.id == "always" and not node.args and not node.keywords):
            return True
        raise AssertionError(f"Unsupported CI expression: {ast.dump(node)}")

    return evaluate(ast.parse(expression, mode="eval").body)


class CiPolicy(unittest.TestCase):
    def context(self, **overrides):
        return {
            "github.repository": "penso/herdr-gpui",
            "github.event_name": "pull_request",
            "github.ref": "refs/pull/456/merge",
            "github.actor": "larry0x",
            "github.triggering_actor": "larry0x",
            "github.event.pull_request.user.login": "larry0x",
            "github.event.pull_request.head.repo.full_name": "larry0x/herdr-gpui",
            "needs.commits.outputs.code": "true",
            **overrides,
        }

    def test_fork_pr_can_run_after_github_approval_or_owner_rerun(self):
        # Approval does not rewrite actor/author/source repo. An owner rerun
        # changes triggering_actor only; neither case may skip the PR checks.
        for actor in ("larry0x", "penso"):
            context = self.context(**{"github.triggering_actor": actor})
            for job in PR_JOBS:
                with self.subTest(actor=actor, job=job):
                    self.assertTrue(condition_matches(job, context))
            self.assertFalse(condition_matches("build", context))

    def test_owner_pr_still_runs_checks_but_not_release_build(self):
        context = self.context(**{
            "github.actor": "penso", "github.triggering_actor": "penso",
            "github.event.pull_request.user.login": "penso",
            "github.event.pull_request.head.repo.full_name": "penso/herdr-gpui",
        })
        for job in PR_JOBS:
            self.assertTrue(condition_matches(job, context), job)
        self.assertFalse(condition_matches("build", context))

    def test_non_pr_events_require_owner_main_and_expected_event(self):
        for event, ref, actor, triggering_actor in itertools.product(
                ("push", "workflow_dispatch", "pull_request_target", "workflow_run"),
                ("refs/heads/main", "refs/heads/topic", "refs/tags/v1"),
                ("penso", "larry0x"), ("penso", "larry0x")):
            context = self.context(**{
                "github.event_name": event, "github.ref": ref,
                "github.actor": actor, "github.triggering_actor": triggering_actor,
            })
            allowed = (event in ("push", "workflow_dispatch") and ref == "refs/heads/main"
                       and actor == "penso" and triggering_actor == "penso")
            for job in JOBS:
                with self.subTest(job=job, context=context):
                    self.assertEqual(condition_matches(job, context), allowed)

    def test_other_repositories_cannot_run_jobs(self):
        for event in ("pull_request", "push", "workflow_dispatch"):
            context = self.context(**{
                "github.repository": "larry0x/herdr-gpui", "github.event_name": event,
                "github.ref": "refs/heads/main", "github.actor": "penso",
                "github.triggering_actor": "penso",
            })
            for job in JOBS:
                self.assertFalse(condition_matches(job, context), (job, event))

    def test_path_selection_only_skips_platform_jobs(self):
        for code in ("false", ""):
            context = self.context(**{"needs.commits.outputs.code": code})
            for job in PR_JOBS:
                self.assertEqual(condition_matches(job, context),
                                 job not in ("checks", "windows"), (job, code))

    def test_required_gate_rejects_failures_skips_and_missing_detection(self):
        # Run the actual shell gate, including the docs-only exception.
        script = JOBS["checks-passed"].split("        run: |\n", 1)[1]
        script = "\n".join(line[10:] for line in script.splitlines() if line.startswith("          "))
        for code, checks, windows, commits in itertools.product(
                ("true", "false", ""), ("success", "failure", "cancelled", "skipped"),
                ("success", "failure", "cancelled", "skipped"), ("success", "failure", "skipped")):
            expected = commits == "success" and (
                (code == "true" and checks == windows == "success")
                or (code == "false" and checks == windows == "skipped"))
            result = subprocess.run(
                ["bash", "-e", "-c", script], capture_output=True,
                env={**os.environ, "CODE": code, "RESULT": checks,
                     "WINDOWS": windows, "COMMITS": commits},
            )
            self.assertEqual(result.returncode == 0, expected, (code, checks, windows, commits))

    def test_ci_has_no_privileged_trigger_or_credentials(self):
        triggers = WORKFLOW.split("\npermissions:", 1)[0]
        self.assertEqual(re.findall(r"^  (\w+):", triggers, re.M),
                         ["push", "pull_request", "workflow_dispatch"])
        self.assertIn("\npermissions: {}\n", WORKFLOW)
        for name, job in JOBS.items():
            with self.subTest(job=name):
                permissions = re.search(r"^    permissions:([^\n]*\n(?:      [^\n]*\n)*)", job, re.M)[1]
                self.assertIn(permissions.strip(), ("{}", "contents: read"))
                for forbidden in ("secrets.", "secrets[", "secrets:", "environment:", "id-token:"):
                    self.assertNotIn(forbidden, job)
                if "actions/checkout@" in job:
                    self.assertIn("persist-credentials: false", job)
                if "Swatinem/rust-cache@" in job:
                    self.assertIn("save-if: ${{ github.ref == 'refs/heads/main' }}", job)
