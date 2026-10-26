#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# == 7 ]] || fail 'Usage: bash package-macos.sh VERSION ARM64_BINARY X86_64_BINARY ARM64_AUTOMATE X86_64_AUTOMATE OUTPUT_DIR THIRD_PARTY_NOTICES'
version_check "$1"
[[ $(uname -s) == Darwin ]] || fail 'macOS required'
[[ -f $2 && -f $3 && -d $6 ]] || fail 'Two binary files and an existing output directory required'
# The automation daemon the app finds beside its executable (automations/fetch.rs).
[[ -f $4 && -f $5 ]] || fail 'Two herdr-automate binary files required'
[[ -f $7 && -s $7 ]] || fail 'Nonempty third-party notices file required'
out=$(cd -- "$6" && pwd)/Herdr.app
new_output "$out"
icons=$(python3 "$release_root/scripts/release/build-icon.py" macos "$2" "$3")
icns=${icons%$'\n'*}
catalog=${icons##*$'\n'}
[[ -f $icns && -f $catalog ]] || fail 'Could not select the macOS icons'
[[ $(lipo -archs "$2") == arm64 ]] || fail 'First binary must be arm64 only'
[[ $(lipo -archs "$3") == x86_64 ]] || fail 'Second binary must be x86_64 only'
[[ $(lipo -archs "$4") == arm64 ]] || fail 'First herdr-automate must be arm64 only'
[[ $(lipo -archs "$5") == x86_64 ]] || fail 'Second herdr-automate must be x86_64 only'
tmp=$(mktemp -d "${out%/*}/.herdr-assembly.XXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
app=$tmp/Herdr.app
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create "$2" "$3" -output "$app/Contents/MacOS/Herdr"
lipo -create "$4" "$5" -output "$app/Contents/MacOS/herdr-automate"
for executable in Herdr herdr-automate; do
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
cp "$7" "$app/Contents/Resources/THIRD-PARTY-NOTICES.txt"
cp "$release_root/crates/herdr-gpui/SOUND-NOTICE.md" "$app/Contents/Resources/"
mv "$app" "$out"
printf '%s\n' "$out"
