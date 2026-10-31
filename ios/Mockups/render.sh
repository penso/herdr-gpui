#!/bin/sh
# Builds the mockups, runs each in a headless simulator, and writes
# screenshots to out/. Usage: ./render.sh [dark|light] [round]
set -eu
cd "$(dirname "$0")"
appearance="${1:-dark}"
round="${2:-2}"
case "$round" in
  1) phone="pair-A-0 pair-A-1 pair-B-0 pair-B-1 pair-C-0 pair-C-1 home-A-0 home-B-0 home-C-0 chat-A-0 chat-B-0 chat-C-0 lock-A-0 lock-B-0 lock-C-0"
     ipad="" ;;
  2) phone="computers-A-0 computers-B-0 computers-C-0 new-A-0 new-B-0 new-C-0 session-A-0 session-B-0 session-C-0 term-A-0 term-B-0 term-C-0 settings-A-0 settings-B-0 settings-C-0"
     ipad="ipad-A-0 ipad-B-0 ipad-C-0" ;;
  3) phone="connect-Now-0 connect-Now-1 connect-A-0 connect-A-1 connect-B-0 connect-B-1 connect-C-0 connect-C-1 connect-D-0 connect-D-1"
     ipad="" ;;
  sync) phone="sync-A-0 sync-A-1 sync-B-0 sync-B-1 sync-C-0 sync-C-1 sync-D-0 sync-D-1"
     ipad="" ;;
  spaces) phone="spaces-A-0 spaces-A-1 spaces-B-0 spaces-B-1 spaces-C-0 spaces-C-1 spaces-D-0 spaces-D-1"
     ipad="" ;;
  mix) phone="spaces-Mix-0 spaces-Mix-1"
     ipad="" ;;
  *) echo "unknown round $round" >&2; exit 2 ;;
esac

udid_of() {
  xcrun simctl list devices available | grep "    $1 (" | head -1 | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/'
}

# render DEVICE SHOTS...
render() {
  device="$1"; shift
  udid=$(udid_of "$device")
  xcodebuild -project HerdrMockups.xcodeproj -scheme HerdrMockups \
    -destination "platform=iOS Simulator,id=$udid" -derivedDataPath build -quiet build
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" >/dev/null
  xcrun simctl ui "$udid" appearance "$appearance"
  xcrun simctl status_bar "$udid" override --time 9:41 --batteryState charged --batteryLevel 100 --wifiBars 3 --cellularBars 4
  xcrun simctl install "$udid" build/Build/Products/Debug-iphonesimulator/HerdrMockups.app
  for shot in "$@"; do
    screen=${shot%%-*}; rest=${shot#*-}; variant=${rest%%-*}; state=${rest#*-}
    xcrun simctl launch --terminate-running-process "$udid" so.pen.herdr-mockups \
      -screen "$screen" -variant "$variant" -state "$state" >/dev/null
    sleep 3
    xcrun simctl io "$udid" screenshot "out/$shot-$appearance.png" >/dev/null 2>&1
  done
}

xcodegen -q
mkdir -p out
# shellcheck disable=SC2086
[ -n "$phone" ] && render "${MOCKUP_DEVICE:-iPhone 17 Pro}" $phone
# shellcheck disable=SC2086
[ -n "$ipad" ] && render "${MOCKUP_IPAD:-iPad Pro 13-inch (M5)}" $ipad
echo "wrote round $round, $appearance"
