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
        source = (ASSETS / "sheprd-avatar.svg").read_text()
        shared = ET.fromstring(source)
        for name, worktree in (("sheprd-linux.svg", False), ("sheprd-linux-worktree.svg", True)):
            with self.subTest(icon=name):
                committed = (ASSETS / name).read_text()
                self.assertEqual(committed, GENERATOR["artwork"](source, "circle", worktree))
                root = ET.fromstring(committed)
                paw = root.find(f"{SVG}g")
                original = shared.find(f"{SVG}g")
                self.assertEqual([(shape.tag, shape.attrib) for shape in paw],
                                 [(shape.tag, shape.attrib) for shape in original])
                self.assertEqual(paw.get("fill"), original.get("fill"))
                self.assertIn("sheprd-avatar.svg", root.find(f"{SVG}metadata").text)
                # No full-canvas background, clip, embedded raster, or external
                # reference: the shell gets a transparent, self-contained disc.
                self.assertEqual([child.tag.removeprefix(SVG) for child in root],
                                  ["title", "desc", "metadata", "defs", "circle", "g"])

    def test_missing_paw_or_gradient_is_rejected(self):
        source = (ASSETS / "sheprd-avatar.svg").read_text()
        for tag in ("path", "defs", "g"):
            root = ET.fromstring(source)
            for parent in root.iter():
                for child in list(parent):
                    if child.tag == SVG + tag:
                        parent.remove(child)
            with self.subTest(missing=tag), self.assertRaises(ValueError):
                GENERATOR["artwork"](ET.tostring(root))
