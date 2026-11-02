#!/bin/sh
# Builds the Mac peripheral into an app bundle (Bluetooth permission needs
# a bundle with a usage description) and runs it, logging to $1.
set -eu
cd "$(dirname "$0")"
log="${1:-/tmp/herdr-ble-spike.log}"
app=build/HerdrBLESpike.app
mkdir -p "$app/Contents/MacOS"
swiftc -O -swift-version 5 Peripheral.swift -o "$app/Contents/MacOS/HerdrBLESpike"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>so.pen.herdr-ble-spike</string>
<key>CFBundleExecutable</key><string>HerdrBLESpike</string>
<key>CFBundleName</key><string>Herdr BLE Spike</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSUIElement</key><true/>
<key>NSBluetoothAlwaysUsageDescription</key><string>The Herdr Bluetooth spike advertises the nearby pairing service.</string>
</dict></plist>
PLIST
codesign --force --sign - "$app"
open -n "$app" --stdout "$log" --stderr "$log"
echo "running; log at $log (quit with: pkill -x HerdrBLESpike)"
