#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# == 5 ]] || fail 'Usage: bash package-macos.sh VERSION ARM64_BINARY X86_64_BINARY OUTPUT_DIR THIRD_PARTY_NOTICES'
version_check "$1"
[[ $(uname -s) == Darwin ]] || fail 'macOS required'
[[ -f $2 && -f $3 && -d $4 ]] || fail 'Two binary files and an existing output directory required'
[[ -f $5 && -s $5 ]] || fail 'Nonempty third-party notices file required'
# The app starts chats with the herdr-acp beside its executable, runs the
# herdr-remote-control beside it as a user service, and runs automations in
# the herdr-automate there; each build leaves all three beside its herdr-gpui.
companions=(herdr-acp herdr-remote-control herdr-automate)
for companion in "${companions[@]}"; do
    [[ -f $(dirname -- "$2")/$companion && -f $(dirname -- "$3")/$companion ]] || fail "A $companion beside each binary required"
done
out=$(cd -- "$4" && pwd)/Herdr.app
new_output "$out"
icons=$(python3 "$release_root/scripts/release/build-icon.py" macos "$2" "$3")
icns=${icons%$'\n'*}
catalog=${icons##*$'\n'}
[[ -f $icns && -f $catalog ]] || fail 'Could not select the macOS icons'
[[ $(lipo -archs "$2") == arm64 ]] || fail 'First binary must be arm64 only'
[[ $(lipo -archs "$3") == x86_64 ]] || fail 'Second binary must be x86_64 only'
for companion in "${companions[@]}"; do
    [[ $(lipo -archs "$(dirname -- "$2")/$companion") == arm64 ]] || fail "First $companion must be arm64 only"
    [[ $(lipo -archs "$(dirname -- "$3")/$companion") == x86_64 ]] || fail "Second $companion must be x86_64 only"
done
tmp=$(mktemp -d "${out%/*}/.herdr-assembly.XXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
app=$tmp/Herdr.app
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create "$2" "$3" -output "$app/Contents/MacOS/Herdr"
for companion in "${companions[@]}"; do
    lipo -create "$(dirname -- "$2")/$companion" "$(dirname -- "$3")/$companion" -output "$app/Contents/MacOS/$companion"
done
for executable in Herdr "${companions[@]}"; do
    for arch in arm64 x86_64; do
        lipo "$app/Contents/MacOS/$executable" -verify_arch "$arch"
    done
    # Rust/linker outputs can contain ad-hoc signatures; distribution signing is separate.
    if codesign -d "$app/Contents/MacOS/$executable" 2>/dev/null; then
        codesign --remove-signature "$app/Contents/MacOS/$executable"
    fi
    chmod 755 "$app/Contents/MacOS/$executable"
done
cp "$release_root/assets/macos/Info.plist" "$app/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$1" "$app/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$1" "$app/Contents/Info.plist"
plutil -insert LSMinimumSystemVersion -string 14.2 "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
cp "$icns" "$app/Contents/Resources/Herdr.icns"
cp "$catalog" "$app/Contents/Resources/Assets.car"
cp "$release_root/crates/herdr-protocol/LICENSE-APACHE" "$app/Contents/Resources/"
cp "$release_root/crates/herdr-protocol/NOTICE.md" "$app/Contents/Resources/"
cp "$release_root/LICENSE" "$release_root/NOTICE" "$release_root/assets/icons/LICENSE-octicons" "$app/Contents/Resources/"
cp "$5" "$app/Contents/Resources/THIRD-PARTY-NOTICES.txt"
cp "$release_root/crates/herdr-gpui/SOUND-NOTICE.md" "$app/Contents/Resources/"
mv "$app" "$out"
printf '%s\n' "$out"
