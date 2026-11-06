#!/bin/sh
# herdr-gpui-managed-shim v1: Sheprd keeps this file up to date. Edit a copy under another name to keep your changes.
#
# Sheprd's `herdr-gpui` command for an agent on an SSH host. Sheprd runs
# on the user's own machine, which this host cannot reach, so a request is
# left as metadata on the caller's Herdr workspace, for a short while; a Herdr
# UI window connected to this host opens the page in that workspace.
set -u
LC_ALL=C
export LC_ALL

# Herdr keeps at most 80 characters per token, 16 tokens per report, and 32
# per workspace, so a request is a header and at most 12 parts of its address.
CHUNK=80
CHUNKS=12
TTL_MS=30000

json=0

usage() {
    cat <<'EOF'
Usage: sheprd browser open [--no-focus] [--tab] URL
       sheprd browser reload

Run on an SSH host, this asks the Sheprd window connected to this host to
open URL in a browser tab of your workspace (HERDR_WORKSPACE_ID).

  open URL      An http or https address, or a bare host such as
                localhost:3000. An address on this host's localhost is
                forwarded to the user's machine first.
  --no-focus    Add the tab without switching to it.
  --tab         Accepted: a page from this host always opens as a tab.
  reload        Reload the tabs this pane opened.
  --json        Print one JSON document.

Only these work from an SSH host. Files on this host cannot be shown: serve
their folder (for example `python3 -m http.server 8000`) and open
localhost:8000. Notes the user sends on a page arrive as your next prompt.

Exit statuses: 0 asked, 1 refused, 2 invalid address, 5 not in a Herdr
pane, 8 not available on an SSH host.
EOF
}

json_string() {
    printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')"
}

fail() {
    status=$1
    code=$2
    shift 2
    if [ "$json" = 1 ]; then
        printf '{"error":{"code":"%s","message":%s}}\n' "$code" "$(json_string "$*")"
    else
        printf '%s\n' "$*" >&2
    fi
    exit "$status"
}

done_message() {
    if [ "$json" = 1 ]; then
        printf '{"message":%s}\n' "$(json_string "$*")"
    else
        printf '%s\n' "$*"
    fi
}

unavailable() {
    fail 8 unsupported "Not available on an SSH host: only \`browser open\` and \`browser reload\` reach Sheprd from here."
}

# send ACTION [URL]: one report, so a window sees the whole request at once.
# The request's ID names its tokens, so requests from this or another pane
# made before a window reads them stand side by side.
send() {
    action=$1
    url=${2:-}
    workspace=${HERDR_WORKSPACE_ID:-}
    [ -n "$workspace" ] || fail 5 not_found "Not running in a Herdr pane: HERDR_WORKSPACE_ID is not set."
    herdr=${HERDR_BIN_PATH:-herdr}
    key="herdr-gpui-$(date +%s)-$$"
    header=$action
    if [ -n "${HERDR_PANE_ID:-}" ]; then
        header="$header $HERDR_PANE_ID"
    fi
    set -- --source herdr-gpui --ttl-ms "$TTL_MS" --token "$key=$header"
    rest=$url
    i=0
    while [ -n "$rest" ] && [ "$i" -lt "$CHUNKS" ]; do
        chunk=$(printf '%s\n' "$rest" | cut -c "1-$CHUNK")
        rest=$(printf '%s\n' "$rest" | cut -c "$((CHUNK + 1))-")
        set -- "$@" --token "${key}_$i=$chunk"
        i=$((i + 1))
    done
    "$herdr" workspace report-metadata "$workspace" "$@" >/dev/null ||
        fail 1 refused "Herdr refused the request; is this a Herdr pane, or are many requests from it still waiting?"
}

open_page() {
    focus=1
    target=
    for arg in "$@"; do
        case $arg in
            --no-focus) focus=0 ;;
            # A page from a host always opens as a tab.
            --tab) ;;
            --beside) fail 8 unsupported "A page opened from an SSH host opens as a tab; --beside is not available here." ;;
            -*) fail 2 usage "Unknown option: $arg" ;;
            *)
                [ -z "$target" ] || fail 2 usage "Give one address."
                target=$arg
                ;;
        esac
    done
    [ -n "$target" ] || fail 2 usage "Give an address to open."
    case $target in
        *://*) ;;
        *)
            if [ -e "$target" ]; then
                fail 8 unsupported "Sheprd cannot show files from an SSH host. Serve the folder, for example with \`python3 -m http.server 8000\`, and open localhost:8000."
            fi
            ;;
    esac
    case $target in
        *://*)
            case $target in
                http://* | https://*) ;;
                *) fail 2 usage "Only http and https addresses open." ;;
            esac
            ;;
    esac
    case $target in
        *[!!-~]*) fail 2 usage "An address has no spaces or control characters." ;;
    esac
    [ "${#target}" -le $((CHUNK * CHUNKS)) ] ||
        fail 2 usage "Addresses longer than $((CHUNK * CHUNKS)) characters cannot be sent from an SSH host."
    if [ "$focus" = 1 ]; then
        send open "$target"
    else
        send open-background "$target"
    fi
    done_message "Asked Sheprd to open $target in workspace $HERDR_WORKSPACE_ID; a window connected to this host opens it."
}

reload_pages() {
    [ "$#" = 0 ] || fail 2 usage "browser reload takes no arguments."
    [ -n "${HERDR_PANE_ID:-}" ] || fail 1 refused "browser reload runs in the Herdr pane that opened the page."
    send reload
    done_message "Asked Sheprd to reload this pane's browser tabs."
}

# Options apply to every command, wherever they appear.
for arg in "$@"; do
    [ "$arg" = --json ] && json=1
done
for arg in "$@"; do
    case $arg in
        --window | --window=*) unavailable ;;
    esac
done
for arg in "$@"; do
    shift
    [ "$arg" = --json ] || set -- "$@" "$arg"
done

case ${1:-} in
    "" | -h | --help | help) usage ;;
    browser)
        shift
        case ${1:-} in
            "" | -h | --help | help) usage ;;
            open)
                shift
                open_page "$@"
                ;;
            reload)
                shift
                reload_pages "$@"
                ;;
            *) unavailable ;;
        esac
        ;;
    *) unavailable ;;
esac
