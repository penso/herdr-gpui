#!/usr/bin/env python3
"""Verify the shipped cross-platform assets derive from the approved paw."""
from pathlib import Path
import runpy
import struct
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets/icons"
SVG = "{http://www.w3.org/2000/svg}"
GENERATOR = runpy.run_path(str(ROOT / "scripts/generate-linux-icons.py"))


class ArtworkTests(unittest.TestCase):
    def test_platform_sources_derive_from_the_canonical_paw(self):
        source = (ASSETS / "sheprd-avatar.svg").read_text()
        variants = {
            "sheprd-linux.svg": ("circle", False, False),
            "sheprd-linux-worktree.svg": ("circle", True, False),
            "sheprd-icon.svg": ("tile", False, False),
            "sheprd-icon-square.svg": ("square", False, False),
            "sheprd-icon-badge.svg": ("tile", False, True),
            "sheprd-github-app.svg": ("square", False, False),
        }
        for name, options in variants.items():
            with self.subTest(name=name):
                self.assertEqual((ASSETS / name).read_text(), GENERATOR["artwork"](source, *options))

    def test_development_colors_preserve_the_cream_foreground(self):
        source = ET.parse(ASSETS / "sheprd-linux-worktree.svg").getroot()
        self.assertEqual(source.find(f"{SVG}g").get("fill"), "#fff8f0")
        self.assertEqual([stop.get("stop-color") for stop in source.iter(f"{SVG}stop")], ["#df4b62", "#991f43"])

    def test_windows_containers_have_all_native_png_sizes(self):
        for name in ("Sheprd.ico", "Sheprd-worktree.ico"):
            data = (ASSETS / name).read_bytes()
            reserved, kind, count = struct.unpack_from("<HHH", data)
            self.assertEqual((reserved, kind), (0, 1))
            sizes = []
            for index in range(count):
                w, h, _, _, planes, depth, length, offset = struct.unpack_from("<BBBBHHII", data, 6 + index * 16)
                sizes.append(w or 256)
                self.assertEqual((w, planes, depth), (h, 1, 32))
                png = data[offset:offset + length]
                self.assertEqual(png[:8], b"\x89PNG\r\n\x1a\n")
                self.assertEqual(struct.unpack_from(">II", png, 16), (w or 256, h or 256))
            self.assertEqual(sizes, [16, 20, 24, 32, 40, 48, 64, 256])

    def test_mobile_icons_are_the_canonical_full_bleed_export(self):
        avatar = (ASSETS / "sheprd-avatar.png").read_bytes()
        for target in ("SheprdRemote", "SheprdRemoteWatch"):
            icon = ROOT / f"ios/{target}/Assets.xcassets/AppIcon.appiconset/AppIcon.png"
            self.assertEqual(icon.read_bytes(), avatar)

    def test_social_cards_have_platform_dimensions_and_fit_upload_limit(self):
        for name, size in (("sheprd-social", (1200, 630)), ("sheprd-repository", (1280, 640))):
            png = (ROOT / f"assets/branding/{name}.png").read_bytes()
            self.assertEqual(struct.unpack_from(">II", png, 16), size)
            self.assertLess(len(png), 1_000_000)


if __name__ == "__main__":
    unittest.main()
