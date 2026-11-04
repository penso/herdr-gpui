#!/usr/bin/env python3
"""Verify XcodeGen's product branding without changing deployed identities.

Run after `xcodegen generate` in ios/. Uses only Python's standard library.
"""
from pathlib import Path
import plistlib
import re
import unittest

IOS = Path(__file__).resolve().parents[1]
ROOT = IOS.parent


def plist(path):
    return plistlib.loads(path.read_bytes())


class BrandingTests(unittest.TestCase):
    def test_app_and_extensions_keep_deployed_identifiers(self):
        spec = (IOS / "project.yml").read_text()
        self.assertEqual(set(re.findall(r"PRODUCT_BUNDLE_IDENTIFIER: (\S+)", spec)), {
            "so.pen.herdr.remote", "so.pen.herdr.remote.watchkitapp",
            "so.pen.herdr.remote.widgets", "so.pen.herdr.remote.share",
            "so.pen.herdr.remote.notifications",
        })
        for suffix in ("", "Watch", "Widgets", "Share", "Notifications"):
            info = plist(IOS / f"SheprdRemote{suffix}/Info.plist")
            self.assertEqual(info["CFBundleDisplayName"], "Sheprd")
            self.assertEqual(info["CFBundleName"], "Sheprd")

    def test_existing_urls_and_nearby_discovery_remain_registered(self):
        info = plist(IOS / "SheprdRemote/Info.plist")
        self.assertIn("herdr-remote", info["CFBundleURLTypes"][0]["CFBundleURLSchemes"])
        self.assertIn("_herdr-remote._tcp", info["NSBonjourServices"])
        watch = plist(IOS / "SheprdRemoteWatch/Info.plist")
        self.assertEqual(watch["WKCompanionAppBundleIdentifier"], "so.pen.herdr.remote")
        for key, value in info.items():
            if key.endswith("UsageDescription"):
                self.assertIn("Sheprd", value)
                self.assertNotIn("Herdr", value)

    def test_extensions_can_still_read_existing_pairings(self):
        for suffix in ("", "Widgets", "Share", "Notifications"):
            name = f"SheprdRemote{suffix}"
            entitlements = plist(IOS / f"{name}/{name}.entitlements")
            self.assertEqual(entitlements["com.apple.security.application-groups"], ["group.so.pen.herdr.remote"])
        keychain = (IOS / "SheprdRemote/Keychain.swift").read_text()
        self.assertIn('static let service = "so.pen.herdr.remote"', keychain)
        self.assertIn('static let group = "group.so.pen.herdr.remote"', keychain)
        notify = (IOS / "Shared/NotifyKey.swift").read_text()
        self.assertIn('kSecAttrService as String: "so.pen.herdr.remote"', notify)

    def test_bluetooth_identities_match_existing_desktops(self):
        helper = (ROOT / "macos/sheprd-nearby/main.swift").read_text()
        for value in (
            "DD750B9A-9568-4FC3-B357-B7515AE46DED",
            "820A3154-F1B2-47C2-B8BE-D89E812464A2",
            "8D5498FD-CFE2-4D2C-99E9-1FDBC2D83892",
            "6E3A4F21-8C0B-4D7E-9A55-2F1C0B7D4E93",
        ):
            self.assertIn(value, helper)
            phone = "\n".join(p.read_text() for p in (IOS / "SheprdRemote/Nearby").glob("*.swift"))
            self.assertIn(value, phone)

    def test_macos_uses_bridge_compatible_executable_and_stable_bundle_id(self):
        info = plist(ROOT / "assets/macos/Info.plist")
        self.assertEqual(info["CFBundleIdentifier"], "so.pen.herdr-gpui")
        self.assertEqual(info["CFBundleExecutable"], "sheprd")
        self.assertEqual(info["CFBundleName"], "Sheprd")
        self.assertEqual(info["CFBundleDisplayName"], "Sheprd")
        self.assertEqual(info["CFBundleIconName"], "Sheprd")


if __name__ == "__main__":
    unittest.main()
