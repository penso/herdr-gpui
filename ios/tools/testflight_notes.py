#!/usr/bin/env python3
"""Writes a TestFlight build's "What to Test" from the commits since the
build testers already have.

App Store Connect is the record of what shipped: each build's notes end in
a `Source: SHA` line, and the next upload reads it back. That covers CI and
local uploads alike and needs no tags or write access to the repository. A
build without the line, or from a commit this checkout lacks (an unpushed
local upload), is skipped for the one before it.

Commit subjects are the entries, sorted as cliff.toml sorts the release
notes. This reads them with `git log` rather than git-cliff so the upload
job needs nothing installed.

Usage: python3 testflight_notes.py compose BUNDLE_ID
       python3 testflight_notes.py publish BUNDLE_ID BUILD NOTES_FILE
"""

import re
import subprocess
import sys
import time
import urllib.error
from pathlib import Path

import app_store_connect as asc

ROOT = Path(__file__).resolve().parents[2]
# What the app is built from: the paths that start an upload in ios.yml.
PATHSPEC = [
    "ios",
    ":(exclude)ios/Mockups",
    ":(exclude,glob)ios/**/*.md",
    "crates/sheprd-remote-control/src/native.rs",
    "crates/sheprd-remote-control/src/native",
]
GROUPS = ["Security", "Added", "Changed", "Fixed"]
HIDDEN = {"build", "chore", "ci", "docs", "style", "test"}
SHOWN = {"feat": "Added", "fix": "Fixed", "perf": "Changed", "refactor": "Changed", "revert": "Changed"}
SUBJECT = re.compile(r"^(?P<type>[a-z]+)(\((?P<scope>[^)]*)\))?(?P<breaking>!)?: (?P<text>.+)$")
PULL_REQUEST = re.compile(r"\s*\(#\d+\)$")
SOURCE = re.compile(r"^Source: ([0-9a-f]{7,40})$", re.MULTILINE)
SHA_LENGTH = 12
# The field's limit in App Store Connect.
MAX_NOTES = 4000
LOCALE = "en-US"
# Commits listed when no earlier build names its source.
FIRST_NOTES_COMMITS = 20
# Older builds tried for a source; past this the history is long gone.
BASELINE_BUILDS = 20
# Processing usually takes minutes, and the upload job's timeout allows this.
PUBLISH_TIMEOUT = 30 * 60
POLL_SECONDS = 30
FAILED_STATES = {"FAILED", "INVALID"}


def entry(subject: str):
    """The (group, text) a commit subject becomes, or None for one testers
    should not see. Anything non-conventional counts as Changed."""
    match = SUBJECT.match(subject)
    if match is None:
        return "Changed", PULL_REQUEST.sub("", subject)
    text = PULL_REQUEST.sub("", match["text"])
    text = text[:1].upper() + text[1:]
    if match["scope"] == "security":
        return "Security", text
    if match["type"] in SHOWN:
        return SHOWN[match["type"]], text
    if match["type"] in HIDDEN and not match["breaking"]:
        return None
    return "Changed", text


def render(groups: dict, omitted: int, source: str) -> str:
    sections = [
        "\n".join([name, *(f"- {text}" for text in groups[name])])
        for name in GROUPS
        if groups.get(name)
    ]
    if omitted:
        sections.append(f"And {omitted} more.")
    if not sections:
        sections.append("No changes for testers since the last build.")
    return "\n\n".join([*sections, f"Source: {source[:SHA_LENGTH]}"])


def compose(subjects: list, source: str, limit: int = MAX_NOTES) -> str:
    """The notes for `subjects`, newest first. Over `limit` the oldest
    entries go, never the `Source:` line the next build depends on."""
    groups = {}
    seen = set()
    for subject in subjects:
        found = entry(subject)
        if found is not None and found not in seen:
            seen.add(found)
            groups.setdefault(found[0], []).append(found[1])
    omitted = 0
    notes = render(groups, omitted, source)
    while len(notes) > limit and any(groups.values()):
        last = next(name for name in reversed(GROUPS) if groups.get(name))
        groups[last].pop()
        omitted += 1
        notes = render(groups, omitted, source)
    return notes


def git(root: Path, *arguments: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(root), *arguments], capture_output=True, text=True
    )


def has_commit(root: Path, sha: str) -> bool:
    return git(root, "rev-parse", "--quiet", "--verify", f"{sha}^{{commit}}").returncode == 0


def subjects_since(root: Path, baseline) -> list:
    """Subjects of the app's commits after `baseline`, newest first; with
    none, the latest few. `baseline..HEAD` also holds when the baseline was
    uploaded from a branch that never merged."""
    revision = [f"{baseline}..HEAD"] if baseline else [f"--max-count={FIRST_NOTES_COMMITS}", "HEAD"]
    result = git(root, "log", "--no-merges", "--format=%s", *revision, "--", *PATHSPEC)
    if result.returncode != 0:
        sys.exit(f"testflight-notes: git log failed: {result.stderr.strip()}")
    return result.stdout.splitlines()


def recorded_source(localizations: list):
    """The source named by a build's notes, in any locale."""
    for item in localizations:
        found = SOURCE.search(item["attributes"].get("whatsNew") or "")
        if found:
            return found[1]
    return None


def find_baseline(app: str, bearer: str, root: Path):
    """The source of the newest build that names one this checkout has."""
    builds = asc.get(
        "builds",
        {
            "filter[app]": app,
            "sort": "-uploadedDate",
            "fields[builds]": "version",
            "limit": BASELINE_BUILDS,
        },
        bearer,
    )
    for build in builds["data"]:
        localizations = asc.get(f"builds/{build['id']}/betaBuildLocalizations", {"limit": 50}, bearer)
        source = recorded_source(localizations["data"])
        if source and has_commit(root, source):
            return source
    return None


def write_notes(build_id: str, text: str, bearer: str) -> None:
    localizations = asc.get(f"builds/{build_id}/betaBuildLocalizations", {"limit": 50}, bearer)
    existing = next(
        (item for item in localizations["data"] if item["attributes"].get("locale") == LOCALE),
        None,
    )
    if existing:
        body = {
            "data": {
                "type": "betaBuildLocalizations",
                "id": existing["id"],
                "attributes": {"whatsNew": text},
            }
        }
        asc.send("PATCH", f"betaBuildLocalizations/{existing['id']}", body, bearer)
        return
    body = {
        "data": {
            "type": "betaBuildLocalizations",
            "attributes": {"locale": LOCALE, "whatsNew": text},
            "relationships": {"build": {"data": {"type": "builds", "id": build_id}}},
        }
    }
    asc.send("POST", "betaBuildLocalizations", body, bearer)


def retryable(error: urllib.error.URLError) -> bool:
    """Whether waiting can help: a build still processing answers 409, and
    Apple's service and the network fail briefly. A refused key will not
    start working."""
    if isinstance(error, urllib.error.HTTPError):
        return error.code in (404, 409, 429) or error.code >= 500
    return True


def publish(bundle_id: str, build: str, text: str, sleep=time.sleep, clock=time.monotonic) -> None:
    """Sets the notes once App Store Connect will take them. Each pass signs
    a new token, as the wait can outlast one."""
    deadline = clock() + PUBLISH_TIMEOUT
    while True:
        try:
            bearer = asc.bearer_from_env()
            app = asc.app_id(bundle_id, bearer)
            if app is None:
                sys.exit(f"testflight-notes: App Store Connect has no app {bundle_id}")
            found = asc.get(
                "builds",
                {"filter[app]": app, "filter[version]": build, "fields[builds]": "processingState"},
                bearer,
            )["data"]
            if found:
                state = found[0]["attributes"].get("processingState")
                if state in FAILED_STATES:
                    sys.exit(f"testflight-notes: build {build} is {state}; no notes set")
                write_notes(found[0]["id"], text, bearer)
                return
        except urllib.error.URLError as error:
            if not retryable(error):
                raise
        if clock() >= deadline:
            sys.exit(f"testflight-notes: gave up setting the notes of build {build}")
        sleep(POLL_SECONDS)


def main() -> None:
    arguments = sys.argv[1:]
    try:
        if len(arguments) == 2 and arguments[0] == "compose":
            bearer = asc.bearer_from_env()
            app = asc.app_id(arguments[1], bearer)
            if app is None:
                sys.exit(f"testflight-notes: App Store Connect has no app {arguments[1]}")
            baseline = find_baseline(app, bearer, ROOT)
            head = git(ROOT, "rev-parse", "HEAD").stdout.strip()
            if not head:
                sys.exit("testflight-notes: no commit at HEAD")
            print(compose(subjects_since(ROOT, baseline), head))
        elif len(arguments) == 4 and arguments[0] == "publish":
            publish(arguments[1], arguments[2], Path(arguments[3]).read_text().strip())
        else:
            sys.exit(__doc__)
    except KeyError as missing:
        sys.exit(f"testflight-notes: {missing.args[0]} is not set")
    except urllib.error.HTTPError as error:
        sys.exit(f"testflight-notes: App Store Connect answered {error.code}: {error.read()[:500]!r}")


if __name__ == "__main__":
    main()
