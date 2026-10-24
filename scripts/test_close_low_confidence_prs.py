"""Regression tests for the closure policy; no GitHub credentials or mutations."""

from contextlib import redirect_stdout
from copy import deepcopy
from datetime import datetime, timedelta, timezone
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "close_prs", Path(__file__).with_name("close-low-confidence-prs.py"),
)
policy = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(policy)

REPOSITORY = "owner/project"
HEAD = "a" * 40
NOW = datetime(2026, 10, 24, 12, tzinfo=timezone.utc)


def summary(score=4, head=HEAD, *, identifier=1, updated=NOW):
    return {
        "id": identifier,
        "user": {"login": policy.BOT, "type": "Bot"},
        "updated_at": updated.isoformat(),
        "body": (
            "<!-- greptile_summary -->\n"
            f"<h2>Confidence Score: {score}/5</h2>\n"
            f"<!-- greptile_confidence_score:{score} -->\n"
            f'<sub>Reviews (1) · Last reviewed commit: ["fix: example"]'
            f"(https://github.com/{REPOSITORY}/commit/{head}) · Reviewed by Greptile</sub>"
        ),
    }


def ci_run(**overrides):
    return {
        "id": 123, "run_attempt": 1, "head_sha": HEAD, "event": "pull_request",
        "pull_requests": [], "status": "completed", "conclusion": "failure",
        **overrides,
    }


class FakeGitHub:
    repository = REPOSITORY

    def __init__(self):
        self.pulls = [{"number": 1}, {"number": 2}]
        self.details = {"state": "open", "merged": False, "head": {"sha": HEAD}}
        self.committed = {"committer": {"date": (NOW - timedelta(hours=49)).isoformat()}}
        self.summaries = [summary()]
        self.runs = []
        self.writes = []
        self.reads = 0
        self.on_pull = lambda: None
        self.on_comment = lambda: None

    def open_pulls(self):
        return deepcopy(self.pulls)

    def pull(self, number):
        self.reads += 1
        self.on_pull()
        return deepcopy(self.details)

    def commit(self, head):
        return self.committed

    def comments(self, number):
        return self.summaries

    def ci_runs(self, head):
        return self.runs

    def comment(self, number, body):
        self.writes.append(("comment", number, body))
        self.on_comment()

    def close(self, number):
        self.writes.append(("close", number))
        self.pulls = [pull for pull in self.pulls if pull["number"] != number]


class ScoreTests(unittest.TestCase):
    def test_all_valid_scores(self):
        for score in range(6):
            with self.subTest(score=score):
                self.assertEqual(policy.latest_score([summary(score)], REPOSITORY, HEAD), score)

    def test_unknown_or_ambiguous_scores_are_skipped(self):
        for score in ("", "6", "-1", "4.5", "04", "unknown", "4/5"):
            with self.subTest(score=score):
                self.assertIsNone(policy.latest_score([summary(score)], REPOSITORY, HEAD))
        for suffix in (
            "\n<!-- greptile_confidence_score:5 -->",
            f'\nLast reviewed commit: [other](https://github.com/{REPOSITORY}/commit/{HEAD})',
        ):
            comment = summary()
            comment["body"] += suffix
            self.assertIsNone(policy.latest_score([comment], REPOSITORY, HEAD))

    def test_only_authenticated_bot_summary_for_current_head(self):
        alterations = [
            {"user": {"login": "contributor", "type": "User"}},
            {"user": {"login": policy.BOT, "type": "User"}},
            {"body": "Confidence Score: 4/5"},
            {"body": None},
            {"updated_at": "invalid"},
            {"updated_at": "2026-10-24T12:00:00"},
            {"body": summary(head="b" * 40)["body"]},
            {"body": summary()["body"].replace(REPOSITORY, "attacker/project")},
            {"body": summary()["body"].replace("<!-- greptile_confidence_score:4 -->", "")},
        ]
        for alteration in alterations:
            with self.subTest(alteration=alteration):
                comment = {**summary(), **alteration}
                self.assertIsNone(policy.latest_score([comment], REPOSITORY, HEAD))

    def test_latest_summary_selected_before_parsing(self):
        old = summary(updated=NOW - timedelta(hours=1))
        for new in (summary(5), summary("pending"), summary(head="b" * 40)):
            for comments in ([old, new], [new, old]):
                with self.subTest(comments=comments):
                    expected = 5 if new["body"] == summary(5)["body"] else None
                    self.assertEqual(policy.latest_score(comments, REPOSITORY, HEAD), expected)

    def test_same_timestamp_uses_newer_comment_id(self):
        self.assertEqual(policy.latest_score(
            [summary(5, identifier=2), summary(4, identifier=1)], REPOSITORY, HEAD,
        ), 5)

    def test_unrelated_comments_do_not_reset_score(self):
        unrelated = {**summary(5), "user": {"login": "contributor", "type": "User"}}
        self.assertEqual(policy.latest_score([summary(), unrelated], REPOSITORY, HEAD), 4)


class CiTests(unittest.TestCase):
    def test_terminal_failure_conclusions(self):
        for conclusion in ("failure", "timed_out", "startup_failure"):
            with self.subTest(conclusion=conclusion):
                self.assertEqual(policy.latest_ci_failure(
                    [ci_run(conclusion=conclusion)], HEAD, 1,
                ), (123, 1, conclusion))

    def test_nonfailures_and_incomplete_runs_are_skipped(self):
        for conclusion in (None, "success", "cancelled", "skipped", "neutral", "action_required", "stale"):
            with self.subTest(conclusion=conclusion):
                self.assertIsNone(policy.latest_ci_failure(
                    [ci_run(conclusion=conclusion)], HEAD, 1,
                ))
        for status in ("queued", "in_progress", "waiting", "pending", "requested"):
            with self.subTest(status=status):
                self.assertIsNone(policy.latest_ci_failure([ci_run(status=status)], HEAD, 1))

    def test_old_heads_other_events_and_other_prs_are_skipped(self):
        for overrides in (
            {"head_sha": "b" * 40}, {"event": "push"}, {"event": "workflow_dispatch"},
            {"pull_requests": [{"number": 99}]},
        ):
            with self.subTest(overrides=overrides):
                self.assertIsNone(policy.latest_ci_failure([ci_run(**overrides)], HEAD, 1))

    def test_explicit_pr_association_and_missing_fork_association(self):
        for associations in ([], [{"number": 1}]):
            self.assertEqual(policy.latest_ci_failure(
                [ci_run(pull_requests=associations)], HEAD, 1,
            ), (123, 1, "failure"))

    def test_newer_run_supersedes_failure_even_while_pending(self):
        old = ci_run()
        for new in (
            ci_run(id=124, conclusion="success"),
            ci_run(id=124, status="queued", conclusion=None),
            ci_run(id=124, conclusion="cancelled"),
        ):
            for runs in ([old, new], [new, old]):
                with self.subTest(runs=runs):
                    self.assertIsNone(policy.latest_ci_failure(runs, HEAD, 1))

    def test_rerun_attempt_supersedes_earlier_failure(self):
        for new in (
            ci_run(run_attempt=2, conclusion="success"),
            ci_run(run_attempt=2, status="in_progress", conclusion=None),
        ):
            self.assertIsNone(policy.latest_ci_failure([new, ci_run()], HEAD, 1))


class ClosureTests(unittest.TestCase):
    def setUp(self):
        self.github = FakeGitHub()

    def run_policy(self, apply=True):
        with redirect_stdout(io.StringIO()):
            policy.run(self.github, apply=apply, now=NOW)

    def test_closes_all_candidates_with_explanation_before_closure(self):
        self.run_policy()
        self.assertEqual([write[:2] for write in self.github.writes], [
            ("comment", 1), ("close", 1), ("comment", 2), ("close", 2),
        ])
        body = self.github.writes[0][2]
        for text in (HEAD, "4/5", "5/5", "48 hours", "reopen", "branch is kept"):
            self.assertIn(text, body)

    def test_dry_run_never_writes(self):
        self.run_policy(apply=False)
        self.assertEqual(self.github.writes, [])

    def test_requires_strictly_more_than_48_hours(self):
        for date in (NOW, NOW + timedelta(days=1), NOW - timedelta(hours=48)):
            with self.subTest(date=date):
                self.github.committed["committer"]["date"] = date.isoformat()
                self.run_policy()
                self.assertEqual(self.github.writes, [])
        self.github.committed["committer"]["date"] = (
            NOW - timedelta(hours=48, seconds=1)
        ).isoformat()
        self.run_policy()
        self.assertTrue(self.github.writes)

    def test_malformed_commit_timestamp_never_closes(self):
        for date in (None, "invalid", "2026-10-20T12:00:00"):
            self.github.committed["committer"]["date"] = date
            self.run_policy()
            self.assertEqual(self.github.writes, [])

    def test_closed_and_merged_prs_are_skipped(self):
        for changes in ({"state": "closed"}, {"merged": True}):
            self.github = FakeGitHub()
            self.github.details.update(changes)
            self.run_policy()
            self.assertEqual(self.github.writes, [])

    def test_five_missing_pending_and_stale_scores_never_close(self):
        for comments in ([], [summary(5)], [summary("pending")], [summary(head="b" * 40)]):
            self.github.summaries = comments
            self.run_policy()
            self.assertEqual(self.github.writes, [])

    def test_ci_failure_closes_even_with_five_or_no_review(self):
        for comments in ([summary(5)], [], [summary("pending")], [summary(head="b" * 40)]):
            with self.subTest(comments=comments):
                self.github = FakeGitHub()
                self.github.summaries = comments
                self.github.runs = [ci_run()]
                self.run_policy()
                self.assertEqual([write[:2] for write in self.github.writes], [
                    ("comment", 1), ("close", 1), ("comment", 2), ("close", 2),
                ])
                body = self.github.writes[0][2]
                self.assertIn("CI run 123", body)
                self.assertIn("failed (failure)", body)
                self.assertIn("/actions/runs/123/attempts/1", body)
                self.assertNotIn("Greptile's confidence", body)

    def test_both_reasons_are_explained(self):
        self.github.runs = [ci_run()]
        self.run_policy()
        body = self.github.writes[0][2]
        self.assertIn("4/5", body)
        self.assertIn("CI run 123", body)

    def test_ci_failure_obeys_same_age_threshold(self):
        self.github.summaries = []
        self.github.runs = [ci_run()]
        self.github.committed["committer"]["date"] = (NOW - timedelta(hours=48)).isoformat()
        self.run_policy()
        self.assertEqual(self.github.writes, [])

    def test_successful_ci_does_not_exempt_low_greptile_score(self):
        self.github.runs = [ci_run(conclusion="success")]
        self.run_policy()
        self.assertTrue(any(write[0] == "close" for write in self.github.writes))

    def test_ci_recovers_before_comment(self):
        self.github.summaries = []
        self.github.runs = [ci_run()]
        def recover():
            if self.github.reads == 2:
                self.github.runs = [ci_run(run_attempt=2, conclusion="success")]
        self.github.on_pull = recover
        self.run_policy()
        self.assertEqual(self.github.writes, [])

    def test_ci_rerun_after_comment_prevents_closure(self):
        self.github.summaries = []
        self.github.runs = [ci_run()]
        def rerun():
            self.github.runs = [ci_run(run_attempt=2, status="in_progress", conclusion=None)]
        self.github.on_comment = rerun
        self.run_policy()
        self.assertEqual([write[:2] for write in self.github.writes], [("comment", 1)])

    def test_drafts_and_recent_comments_do_not_exempt_old_commits(self):
        self.github.details.update(draft=True, updated_at=NOW.isoformat())
        self.run_policy()
        self.assertTrue(any(write[0] == "close" for write in self.github.writes))

    def test_head_change_before_comment_cancels_closure(self):
        def change_head():
            if self.github.reads == 2:
                self.github.details["head"]["sha"] = "b" * 40
        self.github.on_pull = change_head
        self.run_policy()
        self.assertEqual(self.github.writes, [])

    def test_new_score_after_comment_cancels_closure(self):
        def improve():
            self.github.summaries = [summary(5)]
        self.github.on_comment = improve
        self.run_policy()
        self.assertEqual([write[:2] for write in self.github.writes], [("comment", 1)])

    def test_head_change_after_comment_cancels_closure(self):
        def push():
            self.github.details["head"]["sha"] = "b" * 40
        self.github.on_comment = push
        self.run_policy()
        self.assertEqual([write[:2] for write in self.github.writes], [("comment", 1)])

    def test_api_failure_never_closes(self):
        for method in ("comments", "ci_runs", "comment"):
            with self.subTest(method=method), patch.object(
                self.github, method, side_effect=subprocess.CalledProcessError(1, "gh"),
            ):
                with self.assertRaises(subprocess.CalledProcessError):
                    self.run_policy()
                self.assertEqual(self.github.writes, [])


class TransportTests(unittest.TestCase):
    @patch.object(policy.subprocess, "run")
    def test_ci_run_pages_are_flattened_and_scoped_to_ci_workflow_and_head(self, execute):
        execute.return_value.stdout = json.dumps([
            {"total_count": 2, "workflow_runs": [ci_run(id=1)]},
            {"total_count": 2, "workflow_runs": [ci_run(id=2)]},
        ])
        self.assertEqual(policy.GitHub(REPOSITORY).ci_runs(HEAD), [ci_run(id=1), ci_run(id=2)])
        command = execute.call_args.args[0]
        self.assertIn(
            f"repos/{REPOSITORY}/actions/workflows/ci.yml/runs"
            f"?head_sha={HEAD}&event=pull_request&per_page=100", command,
        )
        self.assertIn("--paginate", command)
        self.assertIn("--slurp", command)

    @patch.object(policy.subprocess, "run")
    def test_all_pages_collected_for_pulls_and_comments(self, execute):
        execute.return_value.stdout = json.dumps([[{"id": 1}], [{"id": 2}]])
        github = policy.GitHub(REPOSITORY)
        for call in (github.open_pulls, lambda: github.comments(12)):
            self.assertEqual(call(), [{"id": 1}, {"id": 2}])
            command = execute.call_args.args[0]
            self.assertIn("--paginate", command)
            self.assertIn("--slurp", command)
            self.assertIn("GET", command)

    @patch.object(policy.subprocess, "run")
    def test_comment_body_is_json_input_not_shell_code(self, execute):
        execute.return_value.stdout = "{}"
        body = "$(touch /tmp/unwanted) `false` ' \" \n"
        policy.GitHub(REPOSITORY).comment(1, body)
        self.assertEqual(json.loads(execute.call_args.kwargs["input"]), {"body": body})
        self.assertNotIn(body, execute.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
