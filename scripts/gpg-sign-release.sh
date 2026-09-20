#!/usr/bin/env bash

set -euo pipefail

# GPG-sign published release artifacts with a local key (e.g. YubiKey-resident).
#
# CI signs every artifact with Sigstore, which proves "this workflow, in this
# repository, at this commit, built this file". That is a different claim from
# "the maintainer stands behind this release". This script adds the second one,
# and it runs locally because the signing key is not held by CI and must not be.
#
# Prerequisites:
#   - gh, authenticated with write access to the release
#   - gpg with the signing key available (YubiKey inserted, if applicable)
#
# Usage:
#   ./scripts/gpg-sign-release.sh                 # latest release, default key
#   ./scripts/gpg-sign-release.sh 20260920.01
#   ./scripts/gpg-sign-release.sh -n 20260920.01  # sign locally, do not upload

usage() {
  cat <<'EOF'
Usage: ./scripts/gpg-sign-release.sh [OPTIONS] [VERSION]

Signs published release artifacts with a local GPG key and uploads the
detached .asc signatures back to the GitHub release.

  VERSION   Tag to sign (YYYYMMDD.NN). Defaults to the latest release.

Options:
  -k, --key KEY_ID    GPG key ID or fingerprint to sign with
  -n, --dry-run       Download and sign, but do not upload
  -h, --help          Show this help

Environment:
  GPG_KEY_ID          Default GPG key (overridden by --key)
  HERDR_GPUI_REPO     GitHub repo (default: penso/herdr-gpui)
EOF
}

KEY_ID="${GPG_KEY_ID:-}"
REPO="${HERDR_GPUI_REPO:-penso/herdr-gpui}"
DRY_RUN=false
VERSION=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -k|--key)     KEY_ID="$2"; shift 2 ;;
    -n|--dry-run) DRY_RUN=true; shift ;;
    -h|--help)    usage; exit 0 ;;
    -*)           echo "Unknown option: $1" >&2; usage; exit 1 ;;
    *)            VERSION="$1"; shift ;;
  esac
done

for cmd in gh gpg; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "error: $cmd is required but not found" >&2
    exit 1
  fi
done

if [[ -z "$VERSION" ]]; then
  VERSION="$(gh release view --repo "$REPO" --json tagName --jq '.tagName')"
  echo "Resolved latest release: $VERSION"
fi

if [[ ! "$VERSION" =~ ^[0-9]{8}\.[0-9]{2}$ ]]; then
  echo "error: '$VERSION' does not match YYYYMMDD.NN" >&2
  exit 1
fi

if ! gh release view "$VERSION" --repo "$REPO" --json tagName >/dev/null 2>&1; then
  echo "error: release '$VERSION' not found in $REPO" >&2
  exit 1
fi

# GPG_SIGN_ARGS stays empty when no key is named, so every expansion below needs
# the "${arr[@]+...}" guard: bash 3.2 (macOS's /bin/bash) treats a plain
# "${arr[@]}" on an empty array as unbound under `set -u`.
GPG_SIGN_ARGS=()
if [[ -n "$KEY_ID" ]]; then
  GPG_SIGN_ARGS+=(--local-user "$KEY_ID")
  echo "Using GPG key: $KEY_ID"
else
  if [[ -z "$(gpg --list-secret-keys --keyid-format long 2>/dev/null | head -1 || true)" ]]; then
    echo "error: no GPG secret key found. Specify --key or set GPG_KEY_ID" >&2
    exit 1
  fi
  echo "Using default GPG signing key"
fi

# Probe first so a missing YubiKey fails before anything is downloaded, and so
# the PIN/touch prompt happens once rather than mid-loop.
PROBE="$(mktemp)"
echo "herdr-gpui gpg-sign-release probe" > "$PROBE"
if ! gpg --batch "${GPG_SIGN_ARGS[@]+"${GPG_SIGN_ARGS[@]}"}" --armor --detach-sign "$PROBE" 2>/dev/null; then
  rm -f "$PROBE" "${PROBE}.asc"
  echo "error: GPG signing failed. Is your YubiKey inserted and unlocked?" >&2
  exit 1
fi
rm -f "$PROBE" "${PROBE}.asc"
echo "GPG key verified (signing probe succeeded)"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

echo ""
echo "Downloading release artifacts for $VERSION..."
gh release download "$VERSION" \
  --repo "$REPO" \
  --dir "$WORK_DIR" \
  --pattern '*.app.zip' \
  --pattern '*.tar.gz' \
  --pattern '*.cdx.json' \
  --pattern '*.sha256'

ARTIFACTS=()
while IFS= read -r -d '' f; do
  ARTIFACTS+=("$f")
done < <(find "$WORK_DIR" -maxdepth 1 -type f \
  \( -name '*.app.zip' -o -name '*.tar.gz' -o -name '*.cdx.json' \) \
  -print0 | sort -z)

if [[ ${#ARTIFACTS[@]} -eq 0 ]]; then
  echo "error: no signable artifacts found in release $VERSION" >&2
  exit 1
fi

echo ""
echo "Artifacts to sign (${#ARTIFACTS[@]}):"
for f in "${ARTIFACTS[@]}"; do
  echo "  $(basename "$f")"
done

echo ""
if [[ "$DRY_RUN" == true ]]; then
  echo "Dry run: signing locally only; no .asc files will be uploaded."
else
  echo "Signing and uploading .asc files to release $VERSION."
fi

ASC_FILES=()
for file in "${ARTIFACTS[@]}"; do
  name="$(basename "$file")"
  echo "Signing: $name"

  # Sign what CI published, not a corrupted download: refuse on any mismatch.
  sha256_file="${file}.sha256"
  if [[ -f "$sha256_file" ]]; then
    expected="$(awk '{print $1}' "$sha256_file")"
    actual="$(sha256sum "$file" 2>/dev/null | awk '{print $1}' || shasum -a 256 "$file" | awk '{print $1}')"
    if [[ "$expected" != "$actual" ]]; then
      echo "  ERROR: SHA256 mismatch for $name" >&2
      echo "    expected: $expected" >&2
      echo "    actual:   $actual" >&2
      exit 1
    fi
    echo "  SHA256 verified"
  else
    echo "  ERROR: no .sha256 published for $name; refusing to sign unverified bytes" >&2
    exit 1
  fi

  gpg --batch "${GPG_SIGN_ARGS[@]+"${GPG_SIGN_ARGS[@]}"}" --armor --detach-sign "$file"
  ASC_FILES+=("${file}.asc")
  echo "  Created: ${name}.asc"
done

echo ""
echo "Signed ${#ASC_FILES[@]} artifacts."

if [[ "$DRY_RUN" == true ]]; then
  echo ""
  echo "Dry run — skipping upload. Signatures in: $WORK_DIR"
  echo "To upload manually:"
  echo "  gh release upload $VERSION --repo $REPO ${WORK_DIR}/*.asc"
  trap - EXIT
  exit 0
fi

echo ""
EXISTING_ASC="$(gh release view "$VERSION" --repo "$REPO" --json assets \
  --jq '[.assets[].name | select(endswith(".asc"))] | join(", ")')"
if [[ -n "$EXISTING_ASC" ]]; then
  echo "NOTE: replacing existing signatures: $EXISTING_ASC"
fi

echo "Uploading .asc files to release $VERSION..."
gh release upload "$VERSION" --repo "$REPO" --clobber "${ASC_FILES[@]}"

echo ""
echo "Done. GPG signatures uploaded to release $VERSION."
echo ""
echo "Anyone can now verify with:"
echo "  ./scripts/verify-release.sh --version $VERSION --checksums"
