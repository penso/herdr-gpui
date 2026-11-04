"""Sheprd.app starts the herdr server, so macOS holds the app responsible for
every program in a pane. A privacy-protected resource is denied without a
prompt, and Herdr never appears under Privacy & Security, unless the bundle
carries the resource's usage description and, under the hardened runtime that
release builds are signed with, its entitlement. Programs in panes are often
agents acting for the user, so every resource they could reach is declared."""
from pathlib import Path
import plistlib
import unittest


ROOT = Path(__file__).resolve().parents[3]

# Each hardened-runtime resource entitlement and the usage descriptions that
# macOS requires before it prompts for that resource. These seven are every
# resource entitlement the hardened runtime has, and each grants read and write
# access; the write-only and add-only keys cover narrower requests.
ENTITLED = {
    "com.apple.security.automation.apple-events": ["NSAppleEventsUsageDescription"],
    "com.apple.security.device.audio-input": ["NSMicrophoneUsageDescription"],
    "com.apple.security.device.camera": ["NSCameraUsageDescription"],
    "com.apple.security.personal-information.addressbook": ["NSContactsUsageDescription"],
    "com.apple.security.personal-information.calendars": [
        "NSCalendarsFullAccessUsageDescription",
        "NSCalendarsUsageDescription",
        "NSCalendarsWriteOnlyAccessUsageDescription",
        "NSRemindersFullAccessUsageDescription",
        "NSRemindersUsageDescription",
    ],
    "com.apple.security.personal-information.location": [
        "NSLocationAlwaysAndWhenInUseUsageDescription",
        "NSLocationUsageDescription",
        "NSLocationWhenInUseUsageDescription",
    ],
    "com.apple.security.personal-information.photos-library": [
        "NSPhotoLibraryAddUsageDescription",
        "NSPhotoLibraryUsageDescription",
    ],
}

# Resources that need only a usage description, with no entitlement. Some
# prompt without one, such as the Desktop folder, but their prompt then cannot
# say why; every macOS usage key is declared so none depends on that.
UNENTITLED = [
    "NSAppDataUsageDescription",
    "NSAppleMusicUsageDescription",
    "NSBluetoothAlwaysUsageDescription",
    "NSDesktopFolderUsageDescription",
    "NSDocumentsFolderUsageDescription",
    "NSDownloadsFolderUsageDescription",
    "NSFileProviderDomainUsageDescription",
    "NSGKFriendListUsageDescription",
    "NSLocalNetworkUsageDescription",
    "NSNetworkVolumesUsageDescription",
    "NSRemovableVolumesUsageDescription",
    "NSSpeechRecognitionUsageDescription",
    "NSSystemAdministrationUsageDescription",
    "NSSystemExtensionUsageDescription",
    "NSUserTrackingUsageDescription",
    "NSVideoSubscriberAccountUsageDescription",
]


def load(name):
    return plistlib.loads((ROOT / "assets/macos" / name).read_bytes())


class PrivacyTests(unittest.TestCase):
    def test_every_resource_has_a_usage_description(self):
        info = load("Info.plist")
        keys = [key for keys in ENTITLED.values() for key in keys] + UNENTITLED
        for key in keys:
            with self.subTest(key=key):
                self.assertIsInstance(info.get(key), str)
                self.assertTrue(info[key].strip())

    def test_every_usage_description_is_known(self):
        info = load("Info.plist")
        known = {key for keys in ENTITLED.values() for key in keys} | set(UNENTITLED)
        declared = {key for key in info if key.endswith("UsageDescription")}
        self.assertEqual(declared - known, set())

    def test_release_entitlements_grant_every_resource(self):
        entitlements = load("Sheprd.entitlements")
        self.assertEqual(entitlements, dict.fromkeys(ENTITLED, True))


if __name__ == "__main__":
    unittest.main()
