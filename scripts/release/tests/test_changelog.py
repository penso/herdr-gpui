"""Run: python3 -m unittest discover -s scripts/release/tests -v"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts/release/generate-changelog.sh"
PREAMBLE = "GUI-only release."

MOCK_GIT_CLIFF = """#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["MOCK_LOG"], "a") as log:
    log.write(json.dumps(args) + "\\n")
section = "## [%s] - 2026-09-20\\n\\n### Added\\n- Mock entry\\n" % args[args.index("--tag") + 1]
if "--output" in args:
    with open(args[args.index("--output") + 1], "w") as output:
        output.write("# Changelog\\n\\n" + section)
else:
    sys.stdout.write(section)
"""


def history(work, env, *commits):
    """A scratch repository holding the release scripts and `(subject, tag)` commits,
    so tag selection is tested against known tags rather than the checkout's."""
    repo = work / "repo"
    (repo / "scripts/release").mkdir(parents=True)
    for name in ("generate-changelog.sh", "common.sh"):
        shutil.copy(SCRIPT.parent / name, repo / "scripts/release" / name)
    shutil.copy(ROOT / "cliff.toml", repo / "cliff.toml")
    env = {**env, "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
           "GIT_AUTHOR_NAME": "Test", "GIT_AUTHOR_EMAIL": "test@example.com",
           "GIT_COMMITTER_NAME": "Test", "GIT_COMMITTER_EMAIL": "test@example.com"}

    def git(*args):
        subprocess.run(["git", "-C", str(repo), *args], env=env, capture_output=True, text=True,
                       timeout=30, check=True)

    git("init", "-q")
    for subject, tag in commits:
        git("commit", "-q", "--allow-empty", "-m", subject)
        if tag:
            git("tag", tag)
    return repo / "scripts/release/generate-changelog.sh"


# A stable release, a non-calendar tag, two betas, then the commit being cut.
BETA_HISTORY = (("feat: one", "v20260901.1"), ("feat: foreign", "v1.0"),
                ("feat: two", "v20260905.1"), ("feat: three", "v20260906.2"), ("feat: four", None))


class ChangelogTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="herdr-changelog-test-")
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        self.out = self.work / "out"
        self.out.mkdir()
        self.bin = self.work / "bin"
        self.bin.mkdir()
        mock = self.bin / "git-cliff"
        mock.write_text(MOCK_GIT_CLIFF)
        mock.chmod(0o755)
        self.log = self.work / "calls.jsonl"
        self.env = {"PATH": f"{self.bin}{os.pathsep}{os.environ['PATH']}",
                    "HOME": str(self.work), "MOCK_LOG": str(self.log)}

    def generate(self, version="20260920.3", out=None, success=True, betas=(), script=SCRIPT):
        result = subprocess.run(["bash", str(script), version, str(self.out if out is None else out), *betas],
                                env=self.env, capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode == 0, success, result.stderr)
        return result

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_release_body_leads_with_the_platform_facts_then_the_history(self):
        self.generate()
        notes = (self.out / "RELEASE_NOTES.md").read_text()
        head = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"],
                              capture_output=True, text=True, check=True).stdout.strip()
        self.assertTrue(notes.startswith(PREAMBLE), notes)
        self.assertIn(f"Source: {head}.", notes)
        self.assertIn("## [v20260920.3]", notes)
        self.assertIn("- Mock entry", notes)
        self.assertIn("# Changelog", (self.out / "CHANGELOG.md").read_text())

    def test_the_tag_being_cut_names_the_still_unreleased_commits(self):
        self.generate()
        changelog, notes = self.calls()
        self.assertNotIn("--unreleased", changelog)
        self.assertIn("--unreleased", notes)
        for call in (changelog, notes):
            self.assertEqual(call[call.index("--tag") + 1], "v20260920.3")
            self.assertEqual(call[call.index("--config") + 1], "cliff.toml")
        self.assertIn("--strip", notes)

    def test_unpromoted_betas_fold_into_the_release_after_them(self):
        script = history(self.work, self.env, *BETA_HISTORY)
        self.generate(script=script, betas=("v20260905.1", "v20260906.2"))
        changelog, notes = self.calls()
        for call in (changelog, notes):
            self.assertEqual(call[call.index("--ignore-tags") + 1], r"^(v20260905\.1|v20260906\.2)$")
        # --unreleased would stop at the newest beta, so the notes name a range
        # from the newest calendar tag that is not a beta, skipping both betas
        # and the foreign tag between them.
        self.assertNotIn("--unreleased", notes)
        self.assertEqual(notes[-1], "v20260901.1..HEAD")

    def test_a_promoted_beta_bounds_the_next_release(self):
        script = history(self.work, self.env, *BETA_HISTORY)
        # v20260905.1 was promoted, so GitHub no longer lists it as a prerelease.
        self.generate(script=script, betas=("v20260906.2",))
        self.assertEqual(self.calls()[1][-1], "v20260905.1..HEAD")

    def test_betas_with_no_stable_release_before_them_cover_all_history(self):
        script = history(self.work, self.env, ("feat: one", "v20260905.1"), ("feat: two", None))
        self.generate(script=script, betas=("v20260905.1",))
        notes = self.calls()[1]
        self.assertNotIn("--unreleased", notes)
        self.assertEqual(notes[-2:], ["--strip", "header"], notes)

    def test_malformed_beta_tags_are_refused_before_git_cliff_runs(self):
        for beta in ("20260918.1", "v20260918.01", "v20260918.1|.*", "$(id)", ""):
            with self.subTest(beta=beta):
                self.generate(betas=(beta,), success=False)
        self.assertFalse(self.log.exists())

    def test_bad_version_missing_directory_and_existing_output_refused(self):
        for version in ("v20260920.3", "20260920.3-rc1", "1.2", "020260920.3", "$(id)", ""):
            with self.subTest(version=version):
                self.generate(version=version, success=False)
        self.generate(out=self.work / "absent", success=False)
        self.assertFalse(self.log.exists())
        for name in ("CHANGELOG.md", "RELEASE_NOTES.md"):
            with self.subTest(name=name):
                shutil.rmtree(self.out)
                self.out.mkdir()
                (self.out / name).write_text("previous run\n")
                self.generate(success=False)
                self.assertEqual((self.out / name).read_text(), "previous run\n")

    def test_missing_git_cliff_is_named_rather_than_producing_half_a_release(self):
        (self.bin / "git-cliff").unlink()
        # Still a usable PATH for bash and git; only git-cliff is missing.
        self.env["PATH"] = os.pathsep.join([str(self.bin), "/usr/bin", "/bin"])
        result = self.generate(success=False)
        self.assertIn("git-cliff", result.stderr)
        self.assertEqual(list(self.out.iterdir()), [])


@unittest.skipUnless(shutil.which("git-cliff"), "git-cliff is not installed")
class RealChangelogTests(unittest.TestCase):
    def test_history_renders_conventional_types_into_keep_a_changelog_groups(self):
        with tempfile.TemporaryDirectory(prefix="herdr-changelog-real-") as temp:
            subprocess.run(["bash", str(SCRIPT), "20260920.3", temp],
                           capture_output=True, text=True, timeout=180, check=True)
            changelog = (Path(temp) / "CHANGELOG.md").read_text()
        self.assertIn("# Changelog", changelog)
        self.assertIn("## [v20260920.3]", changelog)
        # Merge commits and skipped types never reach a user-facing changelog.
        self.assertNotIn("Merge pull request", changelog)
        self.assertNotIn("### Chore", changelog)
        # git-cliff gives a commit one section, under its newest tag. A release
        # that failed after tagging can leave an older tag on the same commit
        # (v20260930.1 beside v20260930.2), and that one has no section.
        listing = subprocess.run(
            ["git", "-C", str(ROOT), "tag", "--list", "v20*",
             "--format=%(refname:short) %(if)%(*objectname)%(then)%(*objectname)%(else)%(objectname)%(end)"],
            capture_output=True, text=True, check=True).stdout
        newest = {}
        for tag, commit in (line.split()[:2] for line in listing.splitlines()):
            key = tuple(int(part) for part in tag[1:].split("."))
            if commit not in newest or key > newest[commit][0]:
                newest[commit] = (key, tag)
        for _, tag in newest.values():
            self.assertIn(f"## [{tag}]", changelog)

    def test_stable_notes_list_every_unpromoted_beta_change(self):
        with tempfile.TemporaryDirectory(prefix="herdr-changelog-real-") as temp:
            work = Path(temp)
            (work / "out").mkdir()
            script = history(work, {"PATH": os.environ["PATH"], "HOME": temp}, *BETA_HISTORY)
            subprocess.run(["bash", str(script), "20260920.3", str(work / "out"),
                            "v20260905.1", "v20260906.2"], env={"PATH": os.environ["PATH"], "HOME": temp},
                           capture_output=True, text=True, timeout=180, check=True)
            notes = (work / "out" / "RELEASE_NOTES.md").read_text()
        self.assertIn("## [v20260920.3]", notes)
        for entry in ("- Two", "- Three", "- Four"):
            self.assertIn(entry, notes)
        self.assertNotIn("- One", notes)
        self.assertNotIn("## [v20260906.2]", notes)

    def render(self, *messages):
        probes = [argument for message in messages for argument in ("--with-commit", message)]
        return subprocess.run(["git-cliff", "--config", "cliff.toml", "--unreleased",
                               "--tag", "v20260920.3", "--strip", "header", *probes],
                              cwd=ROOT, capture_output=True, text=True, timeout=180,
                              check=True).stdout.split("## [v20260920.3]")[0]

    def test_the_type_chooses_the_group_and_the_subject_is_the_entry(self):
        rendered = self.render("feat(sidebar): add a pinned section",
                               "fix: reject a truncated link",
                               "refactor: split the projection worker",
                               "docs: explain the socket path",
                               "fix(security): reject oversized frames",
                               "make the thing faster")
        self.assertIn("### Added\n- [sidebar] Add a pinned section", rendered)
        self.assertIn("### Fixed\n- Reject a truncated link", rendered)
        self.assertIn("### Changed\n- Split the projection worker\n- Make the thing faster", rendered)
        self.assertIn("### Security\n- [security] Reject oversized frames", rendered)
        self.assertIn("### Documentation\n- Explain the socket path", rendered)

    def test_housekeeping_is_hidden_unless_it_breaks_something(self):
        rendered = self.render("chore: bump a dependency", "ci: pin an action",
                               "build: raise the deployment target", "style: reformat",
                               "test: cover the parser")
        self.assertNotIn("###", rendered)
        # A breaking change reaches users whatever type carries it, and lands in
        # a real group rather than under a bare `chore` heading.
        for message in ("chore!: remove the deprecated flag",
                        "chore: retire the old config key\n\nBREAKING CHANGE: the key is gone."):
            with self.subTest(message=message):
                breaking = self.render(message)
                self.assertNotIn("### chore", breaking)
                self.assertIn("### Changed\n- **BREAKING:** ", breaking)

    def test_only_the_subject_line_reaches_the_release_body(self):
        rendered = self.render("feat: line one of the subject\n\nRationale users never read.")
        self.assertIn("- Line one of the subject\n", rendered)
        self.assertNotIn("Rationale", rendered)


if __name__ == "__main__":
    unittest.main()
