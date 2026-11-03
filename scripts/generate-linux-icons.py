#!/usr/bin/env python3
"""Derive platform SVGs from Sheprd's canonical paw (standard library only).

Linux uses a circular mask; desktop tiles keep platform-specific margins.
This runs first in `just icons`, before the native raster/package generators.
"""
from copy import deepcopy
from pathlib import Path
import xml.etree.ElementTree as ET

ASSETS = Path(__file__).resolve().parents[1] / "assets/icons"
SVG = "{http://www.w3.org/2000/svg}"
ET.register_namespace("", SVG[1:-1])


def artwork(source, shape="circle", worktree=False, badge=False):
    canonical = ET.fromstring(source)
    defs = deepcopy(canonical.find(f"{SVG}defs"))
    paw = deepcopy(canonical.find(f"{SVG}g"))
    if defs is None or paw is None or len(paw) != 5:
        raise ValueError("Canonical avatar must contain its gradient and five paw shapes")
    if worktree:
        # Recolor only the background: the cream paw stays bright, unlike a
        # luminance remap which turns the foreground into low-contrast red.
        for stop, color in zip(defs.iter(f"{SVG}stop"), ("#df4b62", "#991f43"), strict=True):
            stop.set("stop-color", color)
    icon = ET.Element(f"{SVG}svg", {
        "width": "1024", "height": "1024", "viewBox": "0 0 1024 1024",
        "role": "img", "aria-labelledby": "title desc",
    })
    ET.SubElement(icon, f"{SVG}title", id="title").text = "Sheprd"
    ET.SubElement(icon, f"{SVG}desc", id="desc").text = (
        "Cream paw on a " + ("berry-red development" if worktree else "warm orange") + " gradient."
    )
    ET.SubElement(icon, f"{SVG}metadata").text = "Derived from sheprd-avatar.svg; original Sheprd artwork, Apache-2.0."
    icon.append(defs)
    if shape == "circle":
        ET.SubElement(icon, f"{SVG}circle", cx="512", cy="512", r="448", fill="url(#background)")
    elif shape == "tile":
        ET.SubElement(icon, f"{SVG}rect", x="64", y="64", width="896", height="896", rx="192", fill="url(#background)")
    elif shape == "square":
        ET.SubElement(icon, f"{SVG}rect", width="1024", height="1024", fill="url(#background)")
    else:
        raise ValueError(f"Unknown icon shape: {shape}")
    paw.set("transform", "scale(10.24)" if shape == "square" else "translate(64 64) scale(8.96)")
    icon.append(paw)
    if badge:
        ET.SubElement(icon, f"{SVG}circle", cx="839", cy="185", r="135", fill="#c93647", stroke="#fff8f0", **{"stroke-width": "20"})
        # Outlined numeral, independent of installed fonts.
        ET.SubElement(icon, f"{SVG}path", d="M811 152 L842 130 L842 236 M812 236 H874", fill="none", stroke="#fff8f0", **{"stroke-width": "22", "stroke-linecap": "round", "stroke-linejoin": "round"})
    ET.indent(icon, space="  ")
    return ET.tostring(icon, encoding="unicode") + "\n"


def main():
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
        (ASSETS / name).write_text(artwork(source, *options))
        print(f"Generated assets/icons/{name}")


if __name__ == "__main__":
    main()
