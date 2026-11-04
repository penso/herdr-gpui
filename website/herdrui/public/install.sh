#!/bin/sh
# Installs the latest stable Herdr UI release on Linux through the system
# package manager (APT, DNF or pacman), served at https://herdrui.dev/install.sh:
#
#   curl -fsSL https://herdrui.dev/install.sh | sh
#
# It downloads the package for this machine from GitHub Releases, checks it
# against the release's published SHA-256, and installs it with sudo. It never
# installs, starts or upgrades the Herdr daemon. The package manager owns the
# result, so later updates come from the app's App Updates panel.
set -eu

repo=penso/herdr-gpui

fail() {
  printf 'herdr-ui install: %s\n' "$1" >&2
  exit 1
}

case $(uname -s) in
  Linux) ;;
  Darwin) fail 'on macOS, run: brew install penso/tap/herdr-gpui' ;;
  *) fail "unsupported system $(uname -s); see https://github.com/$repo#install" ;;
esac

case $(uname -m) in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) fail "no Linux package for $(uname -m); see https://github.com/$repo#from-source" ;;
esac

if command -v apt-get >/dev/null 2>&1; then
  ext=deb
elif command -v dnf >/dev/null 2>&1; then
  ext=rpm
elif command -v pacman >/dev/null 2>&1; then
  ext=pkg.tar.zst
else
  fail "no APT, DNF or pacman found; download a tarball from https://github.com/$repo/releases or use Nix: nix run github:$repo"
fi

command -v curl >/dev/null 2>&1 || fail 'curl is required'
command -v sha256sum >/dev/null 2>&1 || fail 'sha256sum is required'

# The latest release is always the newest stable one; betas are prereleases.
latest=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest") ||
  fail 'could not reach GitHub Releases'
version=${latest##*/v}
case $version in
  [1-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9].[1-9]*) ;;
  *) fail "unexpected latest release URL: $latest" ;;
esac

file=Herdr-$version-$arch-unknown-linux-gnu.$ext
base=https://github.com/$repo/releases/download/v$version
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

printf 'Downloading %s\n' "$file"
curl -fsSL --proto '=https' -o "$tmp/$file" "$base/$file" || fail "download failed: $base/$file"
curl -fsSL --proto '=https' -o "$tmp/$file.sha256" "$base/$file.sha256" || fail "download failed: $base/$file.sha256"
(cd "$tmp" && sha256sum -c --status "$file.sha256") || fail "checksum mismatch for $file"

sudo=
if [ "$(id -u)" -ne 0 ]; then
  command -v sudo >/dev/null 2>&1 || fail 'run as root or install sudo'
  sudo=sudo
fi

# APT reads the package as its sandbox user, which cannot enter the private
# temporary directory; let it read the verified file there.
chmod 755 "$tmp"
chmod 644 "$tmp/$file"

case $ext in
  deb) $sudo apt-get install -y "$tmp/$file" ;;
  rpm) $sudo dnf install -y "$tmp/$file" ;;
  pkg.tar.zst) $sudo pacman -U --noconfirm "$tmp/$file" ;;
esac

printf 'Installed Herdr UI %s. Start it from your applications menu or run: herdr-gpui\n' "$version"
if ! command -v herdr >/dev/null 2>&1; then
  printf 'Herdr UI needs the Herdr daemon, which is not on PATH. Install it from https://herdr.dev/\n'
fi
