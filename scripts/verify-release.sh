#!/usr/bin/env bash

set -euo pipefail

# Verify herdr-gpui release artifacts against the maintainer's GPG key.
#
# A release carries three independent claims:
#   - .sha256/.sha512  the bytes are intact
#   - .sig/.crt        Sigstore: this repository's workflow built them
#   - .asc             GPG: the maintainer signed off on them
# This script checks the first and the third. For the second, use:
#   gh attestation verify <file> --repo penso/herdr-gpui
#
# Usage:
#   ./scripts/verify-release.sh --version 20260920.01 --checksums
#   ./scripts/verify-release.sh herdr-gpui-*.app.zip

GPG_KEY_URL="https://pen.so/gpg.asc"
EXPECTED_FINGERPRINT="310320A8CC1C5BA86AD09040C0451BADF7649BBF"
REPO="${HERDR_GPUI_REPO:-penso/herdr-gpui}"

usage() {
  cat <<'EOF'
Usage: ./scripts/verify-release.sh [OPTIONS] [FILE...]

Verifies GPG signatures on herdr-gpui release artifacts.

  FILE          Local artifacts to verify (each needs a matching .asc)

Options:
  -V, --version VER   Download and verify every artifact for this release
  -k, --key URL       GPG public key URL (default: https://pen.so/gpg.asc)
  -s, --skip-key      Skip key import (already in your keyring)
      --checksums     Also verify SHA256 checksums
  -h, --help          Show this help

Environment:
  HERDR_GPUI_REPO     GitHub repo (default: penso/herdr-gpui)
EOF
}

VERSION=""
SKIP_KEY=false
VERIFY_CHECKSUMS=false
FILES=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    -V|--version)  VERSION="$2"; shift 2 ;;
    -k|--key)      GPG_KEY_URL="$2"; shift 2 ;;
    -s|--skip-key) SKIP_KEY=true; shift ;;
    --checksums)   VERIFY_CHECKSUMS=true; shift ;;
    -h|--help)     usage; exit 0 ;;
    -*)            echo "Unknown option: $1" >&2; usage; exit 1 ;;
    *)             FILES+=("$1"); shift ;;
  esac
done

if ! command -v gpg >/dev/null 2>&1; then
  echo "error: gpg is required but not found" >&2
  exit 1
fi

if [[ "$SKIP_KEY" != true ]]; then
  echo "Fetching maintainer GPG key from $GPG_KEY_URL..."
  KEY_DATA="$(curl -fsSL "$GPG_KEY_URL")"
  if [[ -z "$KEY_DATA" ]]; then
    echo "error: failed to fetch GPG key from $GPG_KEY_URL" >&2
    exit 1
  fi

  # Check the fingerprint before the key touches the real keyring: fetching over
  # HTTPS proves the host, not that the host is serving the right key.
  ACTUAL_FINGERPRINT="$(echo "$KEY_DATA" \
    | gpg --with-colons --import-options show-only --import 2>/dev/null \
    | awk -F: '/^fpr/ { print $10; exit }')"
  if [[ "$ACTUAL_FINGERPRINT" != "$EXPECTED_FINGERPRINT" ]]; then
    echo "error: fetched key fingerprint does not match the expected maintainer key" >&2
    echo "  expected: $EXPECTED_FINGERPRINT" >&2
    echo "  actual:   $ACTUAL_FINGERPRINT" >&2
    exit 1
  fi

  echo "$KEY_DATA" | gpg --import 2>&1 || true
  echo ""
fi

WORK_DIR=""
if [[ -n "$VERSION" ]]; then
  if ! command -v gh >/dev/null 2>&1; then
    echo "error: gh (GitHub CLI) is required for --version" >&2
    exit 1
  fi
  if [[ ! "$VERSION" =~ ^[0-9]{8}\.[0-9]{2}$ ]]; then
    echo "error: '$VERSION' does not match YYYYMMDD.NN" >&2
    exit 1
  fi

  WORK_DIR="$(mktemp -d)"
  trap 'rm -rf "$WORK_DIR"' EXIT

  echo "Downloading release artifacts for $VERSION..."
  DL_ARGS=(--pattern '*.app.zip' --pattern '*.tar.gz' --pattern '*.cdx.json' --pattern '*.asc')
  if [[ "$VERIFY_CHECKSUMS" == true ]]; then
    DL_ARGS+=(--pattern '*.sha256')
  fi
  gh release download "$VERSION" --repo "$REPO" --dir "$WORK_DIR" "${DL_ARGS[@]}"

  while IFS= read -r -d '' f; do
    FILES+=("$f")
  done < <(find "$WORK_DIR" -maxdepth 1 -type f \
    \( -name '*.app.zip' -o -name '*.tar.gz' -o -name '*.cdx.json' \) \
    -print0 | sort -z)
fi

if [[ ${#FILES[@]} -eq 0 ]]; then
  echo "error: no files to verify. Provide files or use --version." >&2
  usage
  exit 1
fi

PASS=0
FAIL=0
SKIP=0

for file in "${FILES[@]}"; do
  name="$(basename "$file")"
  asc="${file}.asc"
  echo "Verifying: $name"

  if [[ "$VERIFY_CHECKSUMS" == true ]]; then
    sha256_file="${file}.sha256"
    if [[ -f "$sha256_file" ]]; then
      expected="$(awk '{print $1}' "$sha256_file")"
      actual="$(sha256sum "$file" 2>/dev/null | awk '{print $1}' || shasum -a 256 "$file" | awk '{print $1}')"
      if [[ "$expected" != "$actual" ]]; then
        echo "  FAIL: SHA256 mismatch" >&2
        echo "    expected: $expected" >&2
        echo "    actual:   $actual" >&2
        FAIL=$((FAIL + 1))
        continue
      fi
      echo "  SHA256: OK"
    else
      echo "  SHA256: no checksum file found" >&2
    fi
  fi

  if [[ ! -f "$asc" ]]; then
    echo "  SKIP: no .asc signature found" >&2
    SKIP=$((SKIP + 1))
    continue
  fi

  GPG_OUTPUT="$(gpg --batch --verify "$asc" "$file" 2>&1)" && GPG_RC=0 || GPG_RC=$?
  if [[ $GPG_RC -eq 0 ]]; then
    echo "$GPG_OUTPUT" | grep -i 'good signature' | sed 's/^/  /' || echo "  GPG: OK"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: GPG signature verification failed" >&2
    # shellcheck disable=SC2001 # indenting every line of multi-line output
    echo "$GPG_OUTPUT" | sed 's/^/    /' >&2
    FAIL=$((FAIL + 1))
  fi
done

echo ""
echo "Results: $PASS passed, $FAIL failed, $SKIP skipped (${#FILES[@]} total)"

if [[ $FAIL -gt 0 ]]; then
  echo ""
  echo "ERROR: $FAIL artifact(s) failed verification" >&2
  exit 1
fi

if [[ $PASS -eq 0 ]]; then
  echo ""
  echo "WARNING: nothing was verified (all skipped)" >&2
  exit 1
fi
