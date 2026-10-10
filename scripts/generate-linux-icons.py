#!/usr/bin/env python3
"""Generate circular Linux SVGs from the shared ram artwork (standard library only)."""
from pathlib import Path
import xml.etree.ElementTree as ET

ASSETS = Path(__file__).resolve().parents[1] / "assets/icons"
SVG = "{http://www.w3.org/2000/svg}"
# The red palette matches the flat colors of the macOS worktree PNG.
PALETTES = {
    "herdr-linux.svg": ("#f4f1e9", "#303438"),
    "herdr-linux-worktree.svg": ("#ea6369", "#8e2b38"),
}


def artwork(source, background, foreground):
    root = ET.fromstring(source)
    ram = root.find(f".//{SVG}path[@id='ram']")
    metadata = root.find(f"{SVG}metadata")
    if ram is None or metadata is None:
        raise ValueError("Shared icon must contain the ram path and attribution")
    # Keep the entire silhouette inside the disc, rather than clipping the
    # full-bleed macOS composition. A 64px margin balances the disc's footprint.
    icon = ET.Element(f"{SVG}svg", {
        "width": "1024", "height": "1024", "viewBox": "0 0 1024 1024",
        "role": "img", "aria-labelledby": "title desc",
    })
    ET.SubElement(icon, f"{SVG}title", id="title").text = "Herdr"
    ET.SubElement(icon, f"{SVG}desc", id="desc").text = (
        "Herdr's ram and terminal prompt centered on a circular background."
    )
    ET.SubElement(icon, f"{SVG}metadata").text = metadata.text
    ET.SubElement(icon, f"{SVG}circle", {
        "cx": "512", "cy": "512", "r": "448", "fill": background,
    })
    ET.SubElement(icon, f"{SVG}path", {
        "id": "ram", "d": ram.attrib["d"],
        "transform": "translate(51 792) scale(.15 -.15)", "fill": foreground,
    })
    ET.register_namespace("", SVG[1:-1])
    ET.indent(icon, space="  ")
    return ET.tostring(icon, encoding="unicode") + "\n"


def main():
    source = (ASSETS / "herdr-ui-icon-clean.svg").read_text()
    for name, colors in PALETTES.items():
        (ASSETS / name).write_text(artwork(source, *colors))
        print(f"Generated assets/icons/{name}")


if __name__ == "__main__":
    main()
