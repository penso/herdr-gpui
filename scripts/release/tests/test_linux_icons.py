"""The shipped Linux variants retain the shared logo and can be regenerated."""
from pathlib import Path
import runpy
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[3]
ASSETS = ROOT / "assets/icons"
GENERATOR = runpy.run_path(str(ROOT / "scripts/generate-linux-icons.py"))
SVG = "{http://www.w3.org/2000/svg}"


class LinuxIconTests(unittest.TestCase):
    def test_committed_icons_are_reproducible_and_keep_the_shared_logo(self):
        source = (ASSETS / "herdr-ui-icon-clean.svg").read_text()
        shared = ET.fromstring(source)
        for name, colors in GENERATOR["PALETTES"].items():
            with self.subTest(icon=name):
                committed = (ASSETS / name).read_text()
                self.assertEqual(committed, GENERATOR["artwork"](source, *colors))
                root = ET.fromstring(committed)
                self.assertEqual(root.find(f"{SVG}path").get("d"),
                                 shared.find(f".//{SVG}path[@id='ram']").get("d"))
                self.assertEqual(root.find(f"{SVG}metadata").text,
                                 shared.find(f"{SVG}metadata").text)
                # No full-canvas background, clip, embedded raster, or external
                # reference: the shell gets a transparent, self-contained disc.
                self.assertEqual([child.tag.removeprefix(SVG) for child in root],
                                 ["title", "desc", "metadata", "circle", "path"])

    def test_missing_logo_or_attribution_is_rejected(self):
        source = (ASSETS / "herdr-ui-icon-clean.svg").read_text()
        for tag in ("path", "metadata"):
            root = ET.fromstring(source)
            for parent in root.iter():
                for child in list(parent):
                    if child.tag == SVG + tag:
                        parent.remove(child)
            with self.subTest(missing=tag), self.assertRaises(ValueError):
                GENERATOR["artwork"](ET.tostring(root), "#f4f1e9", "#303438")
