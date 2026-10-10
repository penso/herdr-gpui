#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# -ge 2 ]] || fail 'Usage: bash generate-changelog.sh VERSION OUTPUT_DIR [BETA_TAG...]'
version_check "$1"
[[ -d $2 ]] || fail 'Existing output directory required'
command -v git-cliff >/dev/null || fail 'git-cliff is required: cargo install --locked git-cliff'
out=$(cd -- "$2" && pwd)
version=$1
shift 2

# Unpromoted beta tags fold into the release after them, so a stable release
# (or a beta promoted to one) lists everything since the last stable release
# rather than only what changed since the newest beta.
ignore=()
range=()
if (( $# )); then
    pattern=''
    for beta in "$@"; do
        [[ $beta =~ ^v[1-9][0-9]{7}\.[1-9][0-9]*$ ]] || fail "Beta tag must be vYYYYMMDD.COUNTER: $beta"
        pattern+="${pattern:+|}${beta//./\\.}"
    done
    ignore=(--ignore-tags "^($pattern)\$")
fi
new_output "$out/CHANGELOG.md"
new_output "$out/RELEASE_NOTES.md"

# The tag is created only when the release publishes, so the commits being cut
# are still "unreleased" here and `--tag` names the version they belong to.
tag=v$version
# git-cliff resolves the repository from the working directory; `--workdir`
# misses the commits of a linked worktree, which release rehearsals run from.
cd -- "$release_root"
source_sha=$(git rev-parse HEAD)
if (( ${#ignore[@]} )); then
    # git-cliff's --unreleased stops at the newest tag even when it is ignored,
    # so the notes name their range: from the newest non-beta tag, if any.
    for candidate in $(git tag --list 'v[0-9]*' --merged HEAD --sort=-v:refname); do
        [[ $candidate =~ ^v[1-9][0-9]{7}\.[1-9][0-9]*$ ]] || continue
        for beta in "$@"; do [[ $candidate == "$beta" ]] && continue 2; done
        range=("$candidate..HEAD")
        break
    done
fi

git-cliff --config cliff.toml --tag "$tag" ${ignore[@]+"${ignore[@]}"} --output "$out/CHANGELOG.md"

# The release body keeps the platform and daemon facts every release states,
# then lists what actually changed since the previous tag.
cat > "$out/RELEASE_NOTES.md" <<NOTES
GUI-only release. macOS 14.2+ universal signed/notarized DMG; experimental x86_64 and ARM64 Linux .deb, .rpm, Arch packages and tarballs (glibc 2.39+); experimental, unsigned Windows x86_64 and ARM64 zips without auto-update. Requires an existing Herdr daemon. Source: $source_sha.

NOTES
if (( ${#ignore[@]} )); then
    git-cliff --config cliff.toml --tag "$tag" ${ignore[@]+"${ignore[@]}"} --strip header ${range[@]+"${range[@]}"} >> "$out/RELEASE_NOTES.md"
else
    git-cliff --config cliff.toml --tag "$tag" --unreleased --strip header >> "$out/RELEASE_NOTES.md"
fi

[[ -s $out/CHANGELOG.md && -s $out/RELEASE_NOTES.md ]] || fail 'Generated changelog is empty'
