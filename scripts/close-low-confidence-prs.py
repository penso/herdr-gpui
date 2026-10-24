"""Close stale PRs with a below-5 Greptile score or failed CI for their current head.

Uses the GitHub CLI's authentication. Without --apply, only reports candidates.
"""

import argparse
from datetime import datetime, timedelta, timezone
import json
import re
import subprocess


MAX_AGE = timedelta(hours=48)
BOT = "greptile-apps[bot]"
SUMMARY = "<!-- greptile_summary -->"


def timestamp(value):
    if not isinstance(value, str):
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
        return parsed if parsed.tzinfo is not None else None
    except ValueError:
        return None


def latest_score(comments, repository, head):
    summaries = [
        comment for comment in comments
        if comment.get("user", {}).get("login") == BOT
        and comment.get("user", {}).get("type") == "Bot"
        and SUMMARY in (comment.get("body") or "")
    ]
    if not summaries:
        return None
    dates = [timestamp(comment.get("updated_at")) for comment in summaries]
    if any(date is None for date in dates):
        return None
    # Select before parsing: a new incomplete review must not revive an old score.
    comment = max(zip(dates, summaries), key=lambda item: (item[0], item[1]["id"]))[1]
    body = comment["body"]
    scores = re.findall(r"<!--\s*greptile_confidence_score:([^<>]*?)-->", body)
    if len(scores) != 1 or not re.fullmatch(r"[0-5]", scores[0].strip()):
        return None
    commits = re.findall(
        r"Last reviewed commit: \[[^\n]*?\]\(https://github\.com/"
        + re.escape(repository) + r"/commit/([0-9a-f]{40})\)",
        body,
    )
    if commits != [head]:
        return None
    return int(scores[0].strip())


def latest_ci_failure(runs, head, number):
    matching = [
        run for run in runs
        if run.get("head_sha") == head and run.get("event") == "pull_request"
        # GitHub sometimes omits PR associations, notably for fork runs. The
        # workflow endpoint and exact head SHA still identify the CI result.
        and (not run.get("pull_requests") or any(
            pull.get("number") == number for pull in run["pull_requests"]
        ))
    ]
    if not matching:
        return None
    # Select before testing the conclusion: a newer success or in-flight run
    # supersedes old failures. Reruns retain their run ID and increase attempt.
    run = max(matching, key=lambda item: (item["id"], item["run_attempt"]))
    if run.get("status") != "completed" or run.get("conclusion") not in {
        "failure", "timed_out", "startup_failure",
    }:
        return None
    return run["id"], run["run_attempt"], run["conclusion"]


class GitHub:
    def __init__(self, repository):
        self.repository = repository

    def request(self, path, *, method="GET", payload=None, paginate=False, collection=None):
        command = ["gh", "api", "--hostname", "github.com", path, "--method", method]
        if paginate:
            command += ["--paginate", "--slurp"]
        if payload is not None:
            command += ["--input", "-"]
        result = subprocess.run(
            command, input=json.dumps(payload) if payload is not None else None,
            text=True, capture_output=True, check=True, timeout=120,
        )
        data = json.loads(result.stdout)
        if paginate:
            return [item for page in data for item in (page[collection] if collection else page)]
        return data

    def open_pulls(self):
        return self.request(
            f"repos/{self.repository}/pulls?state=open&per_page=100", paginate=True,
        )

    def pull(self, number):
        return self.request(f"repos/{self.repository}/pulls/{number}")

    def commit(self, head):
        return self.request(f"repos/{self.repository}/git/commits/{head}")

    def comments(self, number):
        return self.request(
            f"repos/{self.repository}/issues/{number}/comments?per_page=100", paginate=True,
        )

    def ci_runs(self, head):
        return self.request(
            f"repos/{self.repository}/actions/workflows/ci.yml/runs"
            f"?head_sha={head}&event=pull_request&per_page=100",
            paginate=True, collection="workflow_runs",
        )

    def close(self, number):
        self.request(
            f"repos/{self.repository}/pulls/{number}", method="PATCH",
            payload={"state": "closed"},
        )

    def comment(self, number, body):
        self.request(
            f"repos/{self.repository}/issues/{number}/comments", method="POST",
            payload={"body": body},
        )


def candidate(github, number, now):
    pull = github.pull(number)
    if pull["state"] != "open" or pull.get("merged"):
        return None
    head = pull["head"]["sha"]
    if not re.fullmatch(r"[0-9a-f]{40}", head):
        return None
    committed = timestamp(github.commit(head).get("committer", {}).get("date"))
    if committed is None or now - committed <= MAX_AGE:
        return None
    score = latest_score(github.comments(number), github.repository, head)
    score = score if score is not None and score < 5 else None
    ci_failure = latest_ci_failure(github.ci_runs(head), head, number)
    if score is None and ci_failure is None:
        return None
    return head, score, ci_failure, committed


def closure_reasons(repository, score, ci_failure):
    reasons = []
    if score is not None:
        reasons.append(f"Greptile's confidence score is **{score}/5**, below the required **5/5**")
    if ci_failure is not None:
        run_id, attempt, conclusion = ci_failure
        reasons.append(
            f"[CI run {run_id}, attempt {attempt}]"
            f"(https://github.com/{repository}/actions/runs/{run_id}/attempts/{attempt}) "
            f"failed ({conclusion})"
        )
    return "; ".join(reasons)


def run(github, *, apply=False, now=None):
    now = now or datetime.now(timezone.utc)
    # Materialize all pages before closing anything; mutation during pagination
    # would shift the open-PR list and skip entries.
    pulls = github.open_pulls()
    for pull in pulls:
        number = pull["number"]
        selected = candidate(github, number, now)
        if selected is None:
            print(f"#{number}: skipped (not old enough or no current low score/CI failure)")
            continue
        head, score, ci_failure, committed = selected
        reasons = closure_reasons(github.repository, score, ci_failure)
        if not apply:
            print(f"#{number}: would close ({reasons}; head {head}, committed {committed.isoformat()})")
            continue
        # Refresh CI, score, and head before acting on the earlier observation.
        if candidate(github, number, now) != selected:
            print(f"#{number}: skipped (PR, review, or CI changed)")
            continue
        # Comment first so a failed comment request cannot leave an unexplained
        # closure. A head/state check follows this additional API round trip.
        github.comment(
            number,
            f"Closing automatically because the current head `{head}` has a committer "
            f"timestamp ({committed.isoformat()}) more than 48 hours old, and: {reasons}.\n\n"
            "Please address the review feedback or CI failures, push your changes, and "
            "reopen the PR when it is ready for another review. The branch is kept.",
        )
        if candidate(github, number, now) != selected:
            print(f"#{number}: skipped (PR, review, or CI changed after commenting)")
            continue
        # GitHub's close endpoint has no conditional-head parameter; the refresh
        # above narrows, but cannot eliminate, concurrent updates during this call.
        github.close(number)
        print(f"#{number}: closed ({reasons})")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, help="GitHub owner/repository")
    parser.add_argument("--apply", action="store_true", help="post comments and close qualifying PRs")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repo):
        parser.error("--repo must be owner/repository")
    run(GitHub(args.repo), apply=args.apply)


if __name__ == "__main__":
    main()
