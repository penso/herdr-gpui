#!/usr/bin/env bash
# Usage: wait.sh FEEDBACK [SECONDS]
# Waits for the mockup window's "Send to agent" and prints the notes.
#
# In a Herdr pane with Herdr GPUI running, the window hands the notes to Herdr
# GPUI, and this waits for them with `herdr-gpui browser feedback --wait`, so
# they are not typed into the pane as well. It also watches FEEDBACK, since
# an older app can accept feedback waits but reject notes.send. This prints
# the fallback file and moves it aside to FEEDBACK.N so the next wait
# sees only the next send. Exits 4 when nothing arrived in time (default
# 600 s).
set -euo pipefail

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: wait.sh FEEDBACK [SECONDS]" >&2
    exit 2
fi
file=$1
seconds=${2:-600}
case "$seconds" in
    '' | *[!0-9]*)
        echo "wait: SECONDS must be a whole number" >&2
        exit 2
        ;;
esac

deadline=$((SECONDS + seconds))
pid=
output=
cleanup() {
    if [ -n "$pid" ]; then
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    fi
    if [ -n "$output" ]; then rm -rf "$output"; fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

finish_feedback() {
    local status=0
    wait "$pid" || status=$?
    pid=
    case "$status" in
        0) cat "$output/notes"; exit 0 ;;
        # Not running, or no socket feedback: the file may still arrive.
        3 | 4) ;;
        # Older apps reject daemon-scoped feedback. Preserve the diagnostic,
        # but keep watching the file their rejected notes.send will produce.
        1) cat "$output/error" >&2 ;;
        *) cat "$output/error" >&2; exit "$status" ;;
    esac
}

# The binary `just mockup` builds; any build answers `browser feedback`.
root=$(cd "$(dirname "$0")/../../../.." && pwd)
app=$root/target/debug/herdr-gpui
if [ -n "${HERDR_PANE_ID:-}" ] && [ -x "$app" ]; then
    output=$(mktemp -d "${TMPDIR:-/tmp}/gpui-feedback.XXXXXX")
    args=(browser feedback)
    if [ "$seconds" -gt 0 ]; then args+=(--wait "$seconds"); fi
    # Keep one continuous socket wait while checking the fallback file.
    "$app" "${args[@]}" >"$output/notes" 2>"$output/error" &
    pid=$!
    if [ "$seconds" -eq 0 ]; then finish_feedback; fi
fi

while :; do
    if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
        finish_feedback
    fi
    if [ -s "$file" ]; then break; fi
    if [ "$SECONDS" -ge "$deadline" ]; then
        echo "wait: no feedback after ${seconds}s" >&2
        exit 4
    fi
    sleep 0.1
done

n=1
while [ -e "$file.$n" ]; do
    n=$((n + 1))
done
# The app replaces the file in one rename, so it is never read half-written.
mv "$file" "$file.$n"
cat "$file.$n"
