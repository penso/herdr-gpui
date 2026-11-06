#!/bin/sh
# Archives Sheprd and uploads it to TestFlight with an App Store
# Connect API key: Xcode signs automatically with the key (no certificates
# or profiles in the repository) and uploads the export itself.
#
# Needs: APPLE_TEAM_ID, ASC_KEY_ID, ASC_ISSUER_ID, and ASC_KEY_P8 (the .p8
# file's contents). BUILD_NUMBER defaults to one more than the highest
# build App Store Connect has for the app (next-build.py). The build's "What
# to Test" lists the commits since the last build (testflight_notes.py), so
# this needs the checkout's full history. Without the key it says so and
# exits 0, so CI passes before the secrets exist.
set -eu
cd "$(dirname "$0")/.."

if [ -z "${ASC_KEY_P8:-}" ] || [ -z "${ASC_KEY_ID:-}" ] || [ -z "${ASC_ISSUER_ID:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ]; then
    echo "TestFlight upload skipped: set APPLE_TEAM_ID, ASC_KEY_ID, ASC_ISSUER_ID, and ASC_KEY_P8."
    exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
key="$work/AuthKey.p8"
umask 077
printf '%s\n' "$ASC_KEY_P8" > "$key"
bundle_id=so.pen.herdr.remote
if [ -z "${BUILD_NUMBER:-}" ]; then
    echo "Asking App Store Connect for the last build number…"
    BUILD_NUMBER=$(python3 tools/next-build.py "$bundle_id")
fi
build=$BUILD_NUMBER
# Before the archive, so a problem here costs seconds, not an upload.
echo "Writing the notes from the commits since the last build…"
python3 tools/testflight_notes.py compose "$bundle_id" > "$work/notes.txt"
cat "$work/notes.txt"
auth="-allowProvisioningUpdates -authenticationKeyPath $key -authenticationKeyID $ASC_KEY_ID -authenticationKeyIssuerID $ASC_ISSUER_ID"

cat > "$work/ExportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>method</key><string>app-store-connect</string>
<key>destination</key><string>upload</string>
<key>teamID</key><string>$APPLE_TEAM_ID</string>
<key>signingStyle</key><string>automatic</string>
<key>manageAppVersionAndBuildNumber</key><false/>
</dict></plist>
PLIST

echo "Generating the Xcode project…"
xcodegen -q
echo "Archiving Sheprd, build $build (several minutes; xcodebuild prints only errors)…"
# shellcheck disable=SC2086
xcodebuild archive -project SheprdRemote.xcodeproj -scheme SheprdRemote \
    -destination 'generic/platform=iOS' -archivePath "$work/SheprdRemote.xcarchive" \
    DEVELOPMENT_TEAM="$APPLE_TEAM_ID" CURRENT_PROJECT_VERSION="$build" \
    $auth -quiet
# The export re-signs with the distribution profile, which sets
# `aps-environment` to production, and uploads to App Store Connect.
echo "Archived. Signing for distribution and uploading to App Store Connect…"
# The export copies with /usr/bin/rsync, which starts its other end as the
# first `rsync` on PATH; Homebrew's rejects the options Apple's passes
# ("--extended-attributes: unknown option"), so the system PATH only.
# shellcheck disable=SC2086
PATH=/usr/bin:/bin:/usr/sbin:/sbin xcodebuild -exportArchive -archivePath "$work/SheprdRemote.xcarchive" \
    -exportOptionsPlist "$work/ExportOptions.plist" -exportPath "$work/export" \
    $auth -quiet
echo "Uploaded build $build. Setting its notes once App Store Connect has it (up to 30 minutes)…"
python3 tools/testflight_notes.py publish "$bundle_id" "$build" "$work/notes.txt"
echo "Build $build has its notes. App Store Connect may still be processing it before TestFlight offers it."
