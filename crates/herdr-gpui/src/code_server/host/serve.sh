# Runs `code serve-web` on a host for Herdr UI's VS Code tabs, on the Unix
# socket in $socket, which the SSH running this forwards to the app. The
# token comes on the first line of stdin, never on a command line; the rest
# of stdin stays open while the app wants the server, and its end stops it.
# Exit codes 3 to 6 say why VS Code could not be found or fetched.
set -u
umask 077
IFS= read -r token || exit 2
dir="$HOME/.herdr-gpui/vscode"
mkdir -p "$dir" || exit 2
token_file="$dir/$(basename "$socket").token"
trap 'rm -f "$socket" "$token_file"' EXIT
printf '%s' "$token" > "$token_file" || exit 2
# VS Code's web build is light until told otherwise; this makes it follow the
# page's light or dark, which the app sets from its theme. Only a host with no
# machine settings yet gets them.
machine="$HOME/.vscode-server/data/Machine"
if [ ! -s "$machine/settings.json" ]; then
  mkdir -p "$machine" &&
    printf '{\n\t"window.autoDetectColorScheme": true\n}\n' > "$machine/settings.json"
fi
# Its own copy of VS Code's command line, never one on PATH: that may be a
# shim that opens an editor elsewhere, or the remote CLI of an SSH session.
code="$dir/code"
if [ ! -x "$code" ]; then
  case "$(uname -s)/$(uname -m)" in
    Linux/x86_64 | Linux/amd64) build=cli-alpine-x64 ;;
    Linux/aarch64 | Linux/arm64) build=cli-alpine-arm64 ;;
    Linux/armv7l | Linux/armv8l) build=cli-linux-armhf ;;
    Darwin/x86_64) build=cli-darwin-x64 ;;
    Darwin/arm64) build=cli-darwin-arm64 ;;
    *) exit 3 ;;
  esac
  url="https://update.code.visualstudio.com/latest/$build/stable"
  if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL --max-time 300 -o "$archive" "$url"; }
  elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -q -T 300 -O "$archive" "$url"; }
  else
    exit 4
  fi
  # Each start downloads into a folder of its own and moves the finished
  # command into place in one rename, so starts at once (two apps, or two
  # names for one account) never write over each other or over a running
  # `code`.
  download=$(mktemp -d "$dir/download.XXXXXX" 2>/dev/null) ||
    { download="$dir/download.$$" && mkdir "$download"; } || exit 2
  archive="$download/cli"
  fetch </dev/null || { rm -rf "$download"; exit 5; }
  # A failed unpack can still leave an executable but partial `code`, which
  # later starts would trust, so nothing from it is moved into place.
  case $build in
    cli-darwin-*) unzip -o -q "$archive" -d "$download" ;;
    *) tar -xzf "$archive" -C "$download" ;;
  esac </dev/null || { rm -rf "$download"; exit 6; }
  if ! [ -x "$download/code" ] || ! mv -f "$download/code" "$code"; then
    rm -rf "$download"
    exit 6
  fi
  rm -rf "$download"
fi
rm -f "$socket"
"$code" serve-web --socket-path "$socket" --connection-token-file "$token_file" \
  --accept-server-license-terms </dev/null 2>&1 &
pid=$!
shell=$$
# serve-web leaves the server it starts running when it stops, and a host
# need not give it a process group of its own, so its whole tree is stopped.
tree() {
  for child in $(ps -A -o pid= -o ppid= | awk -v parent="$1" '$2 == parent { print $1 }'); do
    tree "$child"
  done
  echo "$1"
}
stop() {
  # shellcheck disable=SC2046 # one PID per word
  kill -TERM $(tree "$pid") 2>/dev/null
  rm -f "$socket" "$token_file"
}
# Some SSH servers kill this shell when the connection closes rather than
# end its stdin, so a watcher stops VS Code once the shell is gone.
(
  trap '' HUP
  while kill -0 "$shell" 2>/dev/null; do sleep 2; done
  stop
) </dev/null >/dev/null 2>&1 &
shell_watch=$!
# The end of the input, as the app goes away, stops VS Code. A job started
# in the background reads /dev/null, so the input goes to it as fd 3.
exec 3<&0
(
  cat <&3 >/dev/null
  stop
) </dev/null >/dev/null 2>&1 &
input_watch=$!
exec 3<&-
# Either way this ends once VS Code does, also when it stops or fails on
# its own, so the app sees the SSH end and starts it again.
wait "$pid"
status=$?
stop
kill "$shell_watch" "$input_watch" 2>/dev/null
exit "$status"
