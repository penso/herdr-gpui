"""Independent geometry checks: regeneration alone cannot prove a circular icon."""
from pathlib import Path
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
        path = root.find(f"{SVG}path")
        self.assertIsNotNone(path)
        pair = rf"\(\s*({NUMBER})[\s,]+({NUMBER})\s*\)"
        transform = re.fullmatch(rf"translate{pair}\s+scale{pair}", path.get("transform", ""))
        self.assertIsNotNone(transform, "Update the geometry reader for a new SVG transform syntax")
        tx, ty, sx, sy = map(float, transform.groups())
        points = list(control_points(path.get("d")))
        self.assertTrue(points)
        for x, y in points:
            x, y = tx + sx * x, ty + sy * y
            self.assertLessEqual((x - cx)**2 + (y - cy)**2, radius**2,
                                 f"Ram control point {(x, y)} escapes the circle")

    def test_both_shipped_icons_have_64px_margins_and_contain_the_entire_ram(self):
        for name in ("herdr-linux.svg", "herdr-linux-worktree.svg"):
            with self.subTest(icon=name):
                self.check_geometry(ET.parse(ASSETS / name).getroot())

