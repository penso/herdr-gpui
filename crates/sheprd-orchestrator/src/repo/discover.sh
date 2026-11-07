# Run on the repository's SSH host, with checkout quoted by the caller, and
# `after` defined by it: run with the main checkout once the repository is
# printed (empty for a Beads folder outside Git), it may print more lines.
set -eu
cd -- "$checkout"

# The folder holding the .beads that $1/.beads/redirect names, read as bd reads
# it: the first line neither blank nor a comment, relative to $1. A target that
# cannot be read or is not named .beads prints nothing: bd then follows it.
beads_root() {
    [ -f "$1/.beads/redirect" ] || return 0
    target=$(sed -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' -e 's/^[[:space:]]*//' \
        -e 's/[[:space:]]*$//' -e q "$1/.beads/redirect") || return 0
    [ -n "$target" ] || return 0
    database=$(cd -- "$1" && cd -P -- "$target" && pwd -P) 2>/dev/null || return 0
    case $database in
    */.beads) printf 'beads-root\t%s\n' "${database%/.beads}" ;;
    esac
}

# Missing Git and unreadable or malformed configuration are discovery failures,
# including in folders with .beads. Never turn them into a skipped project.
git config --list >/dev/null
if root=$(git rev-parse --show-toplevel); then
    cd -- "$root"
else
    status=$?
    # Git uses 128 for both absent repositories and real failures. Only discard
    # a folder after proving there is no repository metadata in its ancestry.
    # Keep Git's original stderr and exit status on every uncertain outcome.
    if [ "$status" -ne 128 ] || [ -n "${GIT_DIR:-}" ] || [ -n "${GIT_WORK_TREE:-}" ]; then
        exit "$status"
    fi
    directory=$(pwd -P)
    while :; do
        if [ ! -r "$directory" ] || [ ! -x "$directory" ] ||
            [ -e "$directory/.git" ] || [ -L "$directory/.git" ] ||
            { [ -f "$directory/HEAD" ] && [ -d "$directory/objects" ]; }; then
            exit "$status"
        fi
        [ "$directory" = / ] && break
        directory=${directory%/*}
        directory=${directory:-/}
    done
    if [ -d .beads ]; then
        root=$(pwd -P)
        printf 'common\t%s/.beads\nmain\t%s\nbeads\t1\n' "$root" "$root"
        beads_root "$root"
        after ''
    else
        printf 'not-repository\n'
    fi
    exit 0
fi

common=$(git rev-parse --git-common-dir)
common=$(cd -- "$common" && pwd -P)
# Keep failures out of a pipeline, where sed could mask Git's exit status.
worktrees=$(git worktree list --porcelain)
main=${worktrees%%'
'*}
main=${main#worktree }
printf 'common\t%s\n' "$common"
printf 'main\t%s\n' "${main:-$root}"
if [ -d "${main:-$root}/.beads" ]; then
    printf 'beads\t1\n'
    beads_root "${main:-$root}"
fi
remotes=$(git remote)
for name in $remotes; do
    url=$(git remote get-url "$name")
    printf 'remote\t%s\t%s\n' "$name" "$url"
done
after "${main:-$root}"
