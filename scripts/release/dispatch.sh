#!/usr/bin/env bash
set -euo pipefail

fail() { printf '%s\n' "$*" >&2; exit 1; }

# The workflow derives the calendar version from today's UTC date and the tags
# already published; nothing here chooses a version for a new release. Only a
# promotion names one: the published beta to make stable.
usage='Usage: bash scripts/release/dispatch.sh [--beta | --promote VERSION]'
workflow=release.yml
channel=stable
case "$*" in
    '') ;;
    --beta) channel=beta ;;
    --promote\ *)
        [[ $# == 2 ]] || fail "$usage"
        workflow=promote.yml
        version=$2
        [[ $version =~ ^[1-9][0-9]{7}\.[1-9][0-9]*$ ]] || fail 'Version must be YYYYMMDD.COUNTER (no v prefix)'
        ;;
    *) fail "$usage" ;;
esac
for command in git gh jq openssl; do
    command -v "$command" >/dev/null || fail "Required command: $command"
done

root=$(git rev-parse --show-toplevel)
cd "$root"
[[ -z "$(git status --porcelain --untracked-files=all)" ]] || fail 'Release requires a clean working tree (including untracked files)'
[[ "$(git symbolic-ref --quiet --short HEAD)" == main ]] || fail 'Check out main before releasing'
case "$(git remote get-url origin)" in
    git@github.com:penso/herdr-gpui.git|https://github.com/penso/herdr-gpui.git|https://github.com/penso/herdr-gpui|ssh://git@github.com/penso/herdr-gpui.git) ;;
    *) fail 'origin must be penso/herdr-gpui on github.com' ;;
esac
export GH_HOST=github.com
repo=penso/herdr-gpui
[[ "$(gh api user --jq .login)" == penso ]] || fail 'Only penso may dispatch a release'
sha=$(git rev-parse HEAD)
[[ "$(gh api "repos/$repo/git/ref/heads/main" --jq .object.sha)" == "$sha" ]] || fail 'HEAD does not match origin main on GitHub; no fetch or checkout was performed'

# A nonce avoids confusing this dispatch with another run for the same SHA.
request_id=$(openssl rand -hex 16)
if [[ $workflow == promote.yml ]]; then
    title="Promote v$version [$request_id]"
    gh workflow run promote.yml --repo "$repo" --ref main \
        -f "version=$version" -f "request_id=$request_id"
else
    title="Release $channel @ $sha [$request_id]"
    gh workflow run release.yml --repo "$repo" --ref main \
        -f "expected_sha=$sha" -f "request_id=$request_id" -f "channel=$channel"
fi
printf 'Dispatched %s\n' "$title"

run_id=''
for ((attempt = 0; attempt < 60; attempt++)); do
    runs=$(gh run list --repo "$repo" --workflow "$workflow" --branch main \
        --event workflow_dispatch --limit 100 \
        --json databaseId,displayTitle)
    # Match the input SHA in the title, not headSha: a main-HEAD race must still
    # find our run so we can report its validation failure rather than time out.
    matches=$(jq --arg title "$title" \
        '[.[] | select(.displayTitle == $title)]' <<< "$runs")
    count=$(jq length <<< "$matches")
    [[ "$count" -le 1 ]] || fail "Ambiguous dispatch; inspect Actions for request $request_id"
    if [[ "$count" == 1 ]]; then
        run_id=$(jq -r '.[0].databaseId' <<< "$matches")
        break
    fi
    sleep 2
done
[[ -n "$run_id" ]] || fail "Run not visible after 120s; inspect Actions for request $request_id before retrying"
printf 'Watching https://github.com/%s/actions/runs/%s (protected jobs may await owner approval)\n' "$repo" "$run_id"
gh run watch "$run_id" --repo "$repo" --exit-status
conclusion=$(gh run view "$run_id" --repo "$repo" --json conclusion --jq .conclusion)
[[ "$conclusion" == success ]] || fail "Release run concluded: $conclusion"
if [[ $channel == beta ]]; then
    published=$(gh api "repos/$repo/releases?per_page=10" \
        --jq '[.[] | select(.prerelease and (.draft | not))][0].tag_name' 2>/dev/null || printf 'unknown')
    printf 'Beta %s completed successfully. Promote it with: just release-promote %s\n' \
        "$published" "${published#v}"
else
    published=$(gh api "repos/$repo/releases/latest" --jq .tag_name 2>/dev/null || printf 'unknown')
    printf 'Release %s completed successfully.\n' "$published"
fi
