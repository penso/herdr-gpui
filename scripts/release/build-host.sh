#!/usr/bin/env bash
# The static herdr-remote-control Herdr UI copies onto SSH and cloud machines
# of another platform to pair a phone with them (host-manifest.json, schema
# 4). musl links everything in, so it starts whatever glibc the machine has.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# == 3 ]] || fail 'Usage: bash build-host.sh VERSION TARGET OUTPUT_DIR'
version_check "$1"
case $2 in x86_64-unknown-linux-musl|aarch64-unknown-linux-musl) ;; *) fail 'Unsupported host target';; esac
[[ -d $3 ]] || fail 'Existing output directory required'
arch=${2%%-*}
[[ $(uname -m) == "$arch" ]] || fail "Build $2 on a $arch machine"
out=$(cd -- "$3" && pwd)/herdr-remote-control-$1-$2
new_output "$out"
if ! command -v musl-gcc >/dev/null; then
    sudo apt-get install -y --no-install-recommends musl-tools
fi
rustup target add "$2"
export "CC_${2//-/_}=musl-gcc"
cd "$release_root"
cargo build --locked --release -p herdr-remote-control --target "$2"
binary=target/$2/release/herdr-remote-control
"$binary" --version
# No interpreter to look for on the machine it lands on.
if file -L "$binary" | grep -q 'dynamically linked'; then
    fail 'The host build must be statically linked'
fi
cp "$binary" "$out"
strip "$out"
chmod 755 "$out"
printf '%s\n' "$out"
