#!/usr/bin/env bash
# Builds sheprd-nearby, the Swift helper that is the Mac's Bluetooth side of
# nearby pairing (macos/sheprd-nearby), for one Rust target triple, beside
# that target's release binaries: target/TARGET/release/sheprd-nearby.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# == 1 ]] || fail 'Usage: bash build-nearby.sh TARGET'
[[ $(uname -s) == Darwin ]] || fail 'macOS required'
case $1 in
    aarch64-apple-darwin) arch=arm64 ;;
    x86_64-apple-darwin) arch=x86_64 ;;
    *) fail "Unsupported target: $1" ;;
esac
cd -- "$release_root"
out=target/$1/release
mkdir -p "$out"
xcrun swiftc -O -swift-version 6 -target "$arch-apple-macos${MACOSX_DEPLOYMENT_TARGET:-14.2}" \
    macos/sheprd-nearby/main.swift macos/sheprd-nearby/Framing.swift macos/sheprd-nearby/Bonjour.swift -o "$out/sheprd-nearby"
printf '%s\n' "$out/sheprd-nearby"
