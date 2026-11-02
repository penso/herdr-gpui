#!/usr/bin/env bash
# Install the pinned nfpm release into DESTINATION after checking its SHA-256.
# The digests are copied from nfpm's checksums.txt for this exact version, whose
# Sigstore bundle was verified when the pin was chosen; bump them together.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# == 1 ]] || fail 'Usage: bash install-nfpm.sh DESTINATION_DIR'
version=2.48.0
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) asset=Linux_x86_64 sha=6d7f9a28b2f397934403950fcbba83f598b08d02edbe79a0815b3b4897341750 ;;
    Linux-aarch64) asset=Linux_arm64 sha=a9a75213763783b7669719e505474c748c5ad7dc66591a195619deb50f85c2f4 ;;
    Darwin-arm64) asset=Darwin_arm64 sha=ae23377895437b18d4727b362e7ec4c2d38295eb9adf96008e0cbc813038df81 ;;
    Darwin-x86_64) asset=Darwin_x86_64 sha=be3bdff94bae08638da0f7a3cc4b4ea3f381fa3f10560fcf81ec2e8fea625b49 ;;
    *) fail "No pinned nfpm build for $(uname -s)-$(uname -m)" ;;
esac
mkdir -p -- "$1"
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/nfpm.tar.gz" \
    "https://github.com/goreleaser/nfpm/releases/download/v$version/nfpm_${version}_$asset.tar.gz"
actual=$(python3 -c 'import hashlib, sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "$tmp/nfpm.tar.gz")
[[ $actual == "$sha" ]] || fail "nfpm archive digest mismatch: $actual"
tar -xzf "$tmp/nfpm.tar.gz" -C "$tmp" nfpm
install -m 0755 "$tmp/nfpm" "$1/nfpm"
"$1/nfpm" --version | grep -q "$version" || fail 'Installed nfpm reports an unexpected version'
printf '%s\n' "$1/nfpm"
