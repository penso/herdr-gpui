"""macOS launch paths use Sheprd's bundle and bridge-compatible executable."""
import json
from pathlib import Path
import plistlib
import re
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[3]


class AppNameTests(unittest.TestCase):
    def test_bundle_plist_names_the_app_sheprd(self):
        info = plistlib.loads((ROOT / "assets/macos/Info.plist").read_bytes())
        for key in ("CFBundleName", "CFBundleDisplayName", "CFBundleExecutable"):
            self.assertEqual(info[key], "sheprd" if key == "CFBundleExecutable" else "Sheprd", key)

    @unittest.skipUnless(sys.platform == "darwin", "assetutil requires macOS")
    def test_asset_catalogs_contain_the_plist_icon_name(self):
        # A catalog compiled under another name silently falls back to the
        # flattened .icns, which macOS 26 and later render soft in the Dock.
        name = plistlib.loads((ROOT / "assets/macos/Info.plist").read_bytes())["CFBundleIconName"]
        for catalog in ("Sheprd.car", "Sheprd-worktree.car"):
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

    def test_development_recipes_launch_the_bundled_executable(self):
        justfile = (ROOT / "justfile").read_text()
        for recipe, profile in (("run", "release"), ("run-debug", "debug")):
            body = re.search(rf"(?m)^{re.escape(recipe)} \*args:\n((?:[ \t]+.*\n|\n)+)", justfile)
            self.assertIsNotNone(body, recipe)
            self.assertNotIn(f"target/{profile}/herdr-gpui", body.group(1))
            if recipe == "run":
                # Opened through LaunchServices, so macOS asks for Local
                # Network access instead of denying it to a shell's child.
                self.assertIn('app="$PWD/target/release/Sheprd.app"', body.group(1))
                self.assertIn('"$app" --args {{args}}', body.group(1))
                self.assertRegex(body.group(1), r"\n\s*open -n -W ")
                self.assertIn("{{just_executable()}} bundle release\n", body.group(1))
                self.assertIn("--release -p sheprd-gpui -- {{args}}", body.group(1))
            else:
                launch = f"exec target/{profile}/Sheprd.app/Contents/MacOS/sheprd {{{{args}}}}"
                self.assertIn(launch, body.group(1))

    def test_bundle_recipe_installs_the_executable_and_icon_as_sheprd(self):
        justfile = (ROOT / "justfile").read_text()
        body = re.search(r'(?m)^bundle profile="release" features="":\n((?:[ \t]+.*\n|\n)+)', justfile)
        self.assertIsNotNone(body)
        for expected in (
            'cargo build --locked $flags --target-dir target -p sheprd-gpui --features "{{features}}"',
            'cp target/{{profile}}/sheprd "$app/Contents/MacOS/sheprd"',
            'cp assets/macos/Info.plist "$app/Contents/Info.plist"',
            '"$app/Contents/Resources/Sheprd.icns"',
            '"$app/Contents/Resources/Assets.car"',
        ):
            self.assertIn(expected, body.group(1))
