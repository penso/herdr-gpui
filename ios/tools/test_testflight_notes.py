#!/usr/bin/env python3
"""Tests testflight_notes.py with a scratch repository and a stand-in for
the App Store Connect API. Uses only Python's standard library."""

import io
import subprocess
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest import mock

import testflight_notes as notes

SHA = "0123456789abcdef0123456789abcdef01234567"


def http_error(code):
    return urllib.error.HTTPError("https://example.invalid", code, "error", {}, io.BytesIO(b"{}"))


class EntryTests(unittest.TestCase):
    def test_types_choose_the_group(self):
        self.assertEqual(notes.entry("feat(ios): offer a banner"), ("Added", "Offer a banner"))
        self.assertEqual(notes.entry("fix: stop scans (#12)"), ("Fixed", "Stop scans"))
        self.assertEqual(notes.entry("perf(ios): load faster"), ("Changed", "Load faster"))
        self.assertEqual(notes.entry("fix(security): pin keys"), ("Security", "Pin keys"))

    def test_housekeeping_is_hidden_unless_breaking(self):
        for subject in ("chore: bump", "test(ios): cover it", "docs: explain", "ci: retry"):
            self.assertIsNone(notes.entry(subject))
        self.assertEqual(notes.entry("chore!: drop iOS 25"), ("Changed", "Drop iOS 25"))

    def test_unconventional_subject_counts_as_changed(self):
        self.assertEqual(notes.entry("Rework pairing"), ("Changed", "Rework pairing"))


class ComposeTests(unittest.TestCase):
    def test_groups_and_ends_in_the_source(self):
        text = notes.compose(["fix: b", "chore: hidden", "feat: a", "fix: b"], SHA)
        self.assertEqual(text, f"Added\n- A\n\nFixed\n- B\n\nSource: {SHA[:12]}")

    def test_nothing_for_testers_still_records_the_source(self):
        text = notes.compose(["chore: bump"], SHA)
        self.assertEqual(notes.SOURCE.search(text)[1], SHA[:12])
        self.assertIn("No changes", text)

    def test_over_the_limit_drops_oldest_and_keeps_the_source(self):
        subjects = [f"feat: change number {index}" for index in range(500)]
        text = notes.compose(subjects, SHA)
        self.assertLessEqual(len(text), notes.MAX_NOTES)
        self.assertIn("Change number 0\n", text)
        self.assertNotIn("Change number 499\n", text)
        self.assertRegex(text, r"And \d+ more\.")
        self.assertEqual(notes.SOURCE.search(text)[1], SHA[:12])

    def test_source_survives_a_limit_no_entry_fits(self):
        text = notes.compose(["feat: " + "x" * 200], SHA, limit=60)
        self.assertEqual(text, f"And 1 more.\n\nSource: {SHA[:12]}")


class HistoryTests(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        self.run_git("init", "-q", "-b", "main")
        self.run_git("config", "user.name", "Test")
        self.run_git("config", "user.email", "test@example.invalid")
        self.run_git("config", "commit.gpgsign", "false")

    def run_git(self, *arguments):
        subprocess.run(["git", "-C", str(self.root), *arguments], check=True, capture_output=True)

    def commit(self, path, subject):
        file = self.root / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(subject)
        self.run_git("add", "-A")
        self.run_git("commit", "-q", "--no-verify", "-m", subject)
        return notes.git(self.root, "rev-parse", "HEAD").stdout.strip()

    def test_lists_only_what_the_app_is_built_from(self):
        baseline = self.commit("ios/App.swift", "feat: old")
        self.commit("ios/App.swift", "feat: app")
        self.commit("crates/sheprd-remote-control/src/native/wire.rs", "fix: wire")
        self.commit("crates/sheprd-gpui/src/lib.rs", "feat: desktop")
        self.commit("ios/README.md", "feat: readme")
        self.commit("ios/Mockups/Screen.swift", "feat: mockup")
        self.assertEqual(notes.subjects_since(self.root, baseline), ["fix: wire", "feat: app"])

    def test_without_a_baseline_lists_recent_commits(self):
        self.commit("ios/App.swift", "feat: one")
        self.commit("ios/App.swift", "feat: two")
        self.assertEqual(notes.subjects_since(self.root, None), ["feat: two", "feat: one"])

    def test_baseline_from_an_unmerged_branch(self):
        base = self.commit("ios/App.swift", "feat: base")
        self.run_git("checkout", "-q", "-b", "side")
        side = self.commit("ios/Side.swift", "feat: side")
        self.run_git("checkout", "-q", "main")
        self.commit("ios/App.swift", "feat: main")
        self.assertEqual(notes.subjects_since(self.root, side), ["feat: main"])
        self.assertTrue(notes.has_commit(self.root, base[:12]))

    def test_baseline_skips_builds_without_a_known_source(self):
        known = self.commit("ios/App.swift", "feat: known")
        replies = {
            "builds": {"data": [{"id": "new"}, {"id": "unpushed"}, {"id": "uploaded"}]},
            "builds/new/betaBuildLocalizations": {"data": []},
            "builds/unpushed/betaBuildLocalizations": {
                "data": [{"attributes": {"whatsNew": f"Source: {'f' * 12}"}}]
            },
            "builds/uploaded/betaBuildLocalizations": {
                "data": [{"attributes": {"whatsNew": f"Added\n- A\n\nSource: {known[:12]}"}}]
            },
        }
        with mock.patch.object(notes.asc, "get", lambda path, query, bearer: replies[path]):
            self.assertEqual(notes.find_baseline("app", "token", self.root), known[:12])

    def test_no_source_anywhere(self):
        self.commit("ios/App.swift", "feat: known")
        replies = {
            "builds": {"data": [{"id": "old"}]},
            "builds/old/betaBuildLocalizations": {"data": [{"attributes": {"whatsNew": None}}]},
        }
        with mock.patch.object(notes.asc, "get", lambda path, query, bearer: replies[path]):
            self.assertIsNone(notes.find_baseline("app", "token", self.root))


class FakeApi:
    """App Store Connect with a scripted build and write outcomes."""

    def __init__(self, builds, writes=(), localizations=()):
        self.builds = list(builds)
        self.writes = list(writes)
        self.localizations = list(localizations)
        self.sent = []

    def get(self, path, query, bearer):
        if path == "builds":
            return {"data": self.builds.pop(0) if len(self.builds) > 1 else self.builds[0]}
        return {"data": self.localizations}

    def send(self, method, path, body, bearer):
        if self.writes:
            raise self.writes.pop(0)
        self.sent.append((method, path, body))
        return {}


class PublishTests(unittest.TestCase):
    def publish(self, api):
        self.slept = 0

        def sleep(seconds):
            self.slept += seconds

        with mock.patch.multiple(
            notes.asc,
            bearer_from_env=lambda: "token",
            app_id=lambda bundle_id, bearer: "app",
            get=api.get,
            send=api.send,
        ):
            notes.publish("bundle", "7", "text", sleep=sleep, clock=lambda: self.slept)

    def test_creates_notes_when_the_build_has_none(self):
        api = FakeApi([[{"id": "b", "attributes": {"processingState": "VALID"}}]])
        self.publish(api)
        method, path, body = api.sent[0]
        self.assertEqual((method, path), ("POST", "betaBuildLocalizations"))
        self.assertEqual(body["data"]["attributes"], {"locale": "en-US", "whatsNew": "text"})
        self.assertEqual(body["data"]["relationships"]["build"]["data"]["id"], "b")

    def test_replaces_notes_the_build_already_has(self):
        api = FakeApi(
            [[{"id": "b", "attributes": {"processingState": "VALID"}}]],
            localizations=[{"id": "l", "attributes": {"locale": "en-US"}}],
        )
        self.publish(api)
        method, path, body = api.sent[0]
        self.assertEqual((method, path), ("PATCH", "betaBuildLocalizations/l"))
        self.assertEqual(body["data"]["attributes"], {"whatsNew": "text"})

    def test_waits_for_the_build_and_retries_while_it_processes(self):
        processing = [{"id": "b", "attributes": {"processingState": "PROCESSING"}}]
        api = FakeApi([[], processing], writes=[http_error(409), http_error(503)])
        self.publish(api)
        self.assertEqual(len(api.sent), 1)
        self.assertEqual(self.slept, 3 * notes.POLL_SECONDS)

    def test_gives_up_at_the_deadline(self):
        with self.assertRaises(SystemExit) as stopped:
            self.publish(FakeApi([[]]))
        self.assertIn("gave up", str(stopped.exception))
        self.assertGreaterEqual(self.slept, notes.PUBLISH_TIMEOUT)

    def test_stops_on_a_failed_build(self):
        api = FakeApi([[{"id": "b", "attributes": {"processingState": "INVALID"}}]])
        with self.assertRaises(SystemExit) as stopped:
            self.publish(api)
        self.assertIn("INVALID", str(stopped.exception))
        self.assertEqual(api.sent, [])

    def test_refused_key_is_not_retried(self):
        api = FakeApi(
            [[{"id": "b", "attributes": {"processingState": "VALID"}}]], writes=[http_error(403)]
        )
        with self.assertRaises(urllib.error.HTTPError):
            self.publish(api)
        self.assertEqual(self.slept, 0)


if __name__ == "__main__":
    unittest.main()
