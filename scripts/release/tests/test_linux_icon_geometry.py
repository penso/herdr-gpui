"""Independent geometry checks: regeneration alone cannot prove a circular icon."""
from pathlib import Path
import math
import re
import unittest
import xml.etree.ElementTree as ET

ASSETS = Path(__file__).resolve().parents[3] / "assets/icons"
SVG = "{http://www.w3.org/2000/svg}"
NUMBER = r"[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?"


def control_points(path):
    """Read the logo's move/line/cubic/close commands; reject new syntax loudly.

    Cubic Beziers lie in the convex hull of their four control points. Checking
    every point (including segment starts) inside a convex disc proves the whole
    filled silhouette fits, without renderer dependencies or pixel sampling gaps.
    """
    tokens = re.findall(rf"{NUMBER}|[a-zA-Z]", path)
    if "".join(tokens) != re.sub(r"[\s,]", "", path):
        raise ValueError("Unrecognized SVG path syntax")
    position = start = (0.0, 0.0)
    command = None
    index = 0
    while index < len(tokens):
        if tokens[index].isalpha():
            command = tokens[index]
            index += 1
            if command in "Zz":
                position = start
                yield position
                command = None
                continue
        if command not in ("M", "m", "L", "l", "C", "c"):
            raise ValueError(f"Unsupported SVG path command: {command}")
        count = 6 if command in "Cc" else 2
        values = [float(token) for token in tokens[index:index + count]]
        if len(values) != count:
            raise ValueError("Incomplete SVG path command")
        index += count
        origin = position if command.islower() else (0.0, 0.0)
        points = [(origin[0] + values[i], origin[1] + values[i + 1])
                  for i in range(0, count, 2)]
        yield from points
        position = points[-1]
        if command in "Mm":
            start = position
            command = "l" if command.islower() else "L"


class LinuxIconGeometryTests(unittest.TestCase):
    def check_geometry(self, root):
        self.assertEqual(root.get("viewBox"), "0 0 1024 1024")
        self.assertEqual((root.get("width"), root.get("height")), ("1024", "1024"))
        circle = root.find(f"{SVG}circle")
        self.assertIsNotNone(circle)
        cx, cy, radius = (float(circle.get(key)) for key in ("cx", "cy", "r"))
        # Independent design contract: 64px of transparency on all four sides.
        self.assertEqual((cx - radius, cy - radius, 1024 - cx - radius,
                          1024 - cy - radius), (64, 64, 64, 64))
        paw = root.find(f"{SVG}g")
        self.assertIsNotNone(paw)
        pair = rf"\(\s*({NUMBER})[\s,]+({NUMBER})\s*\)"
        transform = re.fullmatch(rf"translate{pair}\s+scale\(({NUMBER})\)", paw.get("transform", ""))
        self.assertIsNotNone(transform, "Update the geometry reader for a new SVG transform syntax")
        tx, ty, scale = map(float, transform.groups())
        self.assertGreater(scale, 0)
        self.assertTrue(len(paw))
        for shape in paw:
            if shape.tag == SVG + "path":
                self.assertIsNone(shape.get("transform"))
                points = list(control_points(shape.get("d")))
                self.assertTrue(points)
                for x, y in points:
                    x, y = tx + scale * x, ty + scale * y
                    self.assertLessEqual(math.hypot(x - cx, y - cy), radius,
                                         f"Paw control point {(x, y)} escapes the circle")
            else:
                self.assertEqual(shape.tag, SVG + "ellipse")
                x, y, rx, ry = (float(shape.get(key)) for key in ("cx", "cy", "rx", "ry"))
                rotation = re.fullmatch(rf"rotate\(({NUMBER}) ({NUMBER}) ({NUMBER})\)", shape.get("transform", ""))
                self.assertIsNotNone(rotation)
                self.assertEqual(tuple(map(float, rotation.groups()[1:])), (x, y))
                self.assertGreater(min(rx, ry), 0)
                # A rotated ellipse fits inside its circumscribed disc.
                self.assertLessEqual(math.hypot(tx + scale * x - cx, ty + scale * y - cy)
                                     + scale * max(rx, ry), radius)

    def test_both_shipped_icons_have_64px_margins_and_contain_the_entire_paw(self):
        for name in ("sheprd-linux.svg", "sheprd-linux-worktree.svg"):
            with self.subTest(icon=name):
                self.check_geometry(ET.parse(ASSETS / name).getroot())

    def test_geometry_rejects_regenerated_but_broken_layouts(self):
        for tag, attribute, value in (
            ("circle", "r", "512"),
            ("circle", "cx", "540"),
            ("g", "transform", "translate(400 792) scale(8.96)"),
            ("g", "transform", "translate(64 64) scale(30)"),
            ("ellipse", "rx", "100"),
            ("path", "d", "M 0 0 C 20000 0 20000 0 0 0 Z"),
        ):
            root = ET.parse(ASSETS / "sheprd-linux.svg").getroot()
            root.find(".//" + SVG + tag).set(attribute, value)
            with self.subTest(attribute=attribute, value=value), self.assertRaises(AssertionError):
                self.check_geometry(root)

    def test_control_points_handle_relative_curves_repeated_segments_and_close(self):
        self.assertEqual(list(control_points(
            "M 10 20 30 40 c 1 2 3 4 5 6 1 2 3 4 5 6 z m 2 3 l 4 5 "
            "L 1 2 C 3 4 5 6 7 8 Z"
        )), [(10, 20), (30, 40), (31, 42), (33, 44), (35, 46),
             (36, 48), (38, 50), (40, 52), (10, 20), (12, 23), (16, 28),
             (1, 2), (3, 4), (5, 6), (7, 8), (12, 23)])

    def test_control_points_reject_unsupported_or_incomplete_commands(self):
        for path in ("M 0 0 Q 1 2 3 4", "M 0 0 L 1", "M 0 0 ! L 1 2"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                list(control_points(path))
