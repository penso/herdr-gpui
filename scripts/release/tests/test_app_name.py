"""The compiled macOS icon catalogs match the bundle's declared Dock icon."""
import json
from pathlib import Path
import plistlib
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[3]


class AppNameTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "darwin", "assetutil requires macOS")
    def test_asset_catalogs_contain_the_plist_icon_name(self):
        # A catalog compiled under another name silently falls back to the
        # flattened .icns, which macOS 26 and later render soft in the Dock.
        name = plistlib.loads((ROOT / "assets/macos/Info.plist").read_bytes())["CFBundleIconName"]
        for catalog in ("Herdr.car", "Herdr-worktree.car"):
            info = subprocess.run(
                ["xcrun", "assetutil", "--info", ROOT / "assets/icons" / catalog],
                check=True, capture_output=True,
            ).stdout
            # Older assetutil releases list only the flattened renditions that
            # macOS 14 and 15 read, not the layered Icon Composer stack.
            icons = {
                item.get("Name") for item in json.loads(info)
                if item.get("AssetType") in ("IconImageStack", "MultiSized Image", "Icon Image")
            }
            self.assertEqual(icons, {name}, catalog)

