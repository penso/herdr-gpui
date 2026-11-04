#!/usr/bin/env python3
"""Generate the Windows executable icons from the shared tile artwork.

Standard library only; each size is rendered from the vector by rsvg-convert
and packed as a PNG-encoded entry of a multi-size ICO file.
"""
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile

ASSETS = Path(__file__).resolve().parents[1] / "assets/icons"
SIZES = (16, 20, 24, 32, 40, 48, 64, 256)
# Keep the cream paw, recoloring only the gradient for development builds.
VARIANTS = {
    "Sheprd.ico": {},
    "Sheprd-worktree.ico": {
        "#f39a5a": "#df4b62",
        "#d65f2a": "#991f43",
    },
}


def recolor(source, replacements):
    for old, new in replacements.items():
        count = source.count(old)
        if count != 1:
            raise SystemExit(f"Expected {old!r} once in sheprd-icon.svg, found it {count} times")
        source = source.replace(old, new)
    return source


def render(svg, size, scratch):
    output = scratch / f"{size}.png"
    subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size), "-o", str(output), str(svg)],
        check=True,
    )
    return output.read_bytes()


def pack(images):
    """Return an ICO holding each (size, png) pair as a 32-bit PNG entry."""
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = len(header) + 16 * len(images)
    entries, payload = b"", b""
    for size, png in images:
        dimension = 0 if size == 256 else size
        entries += struct.pack("<BBBBHHII", dimension, dimension, 0, 0, 1, 32, len(png), offset)
        payload += png
        offset += len(png)
    return header + entries + payload


def main():
    if shutil.which("rsvg-convert") is None:
        raise SystemExit("rsvg-convert not found: install librsvg (apt-get install librsvg2-bin, brew install librsvg)")
    source = (ASSETS / "sheprd-icon.svg").read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory() as scratch:
        scratch = Path(scratch)
        for name, replacements in VARIANTS.items():
            svg = scratch / "icon.svg"
            svg.write_text(recolor(source, replacements), encoding="utf-8", newline="\n")
            images = [(size, render(svg, size, scratch)) for size in SIZES]
            (ASSETS / name).write_bytes(pack(images))
            print(f"Generated assets/icons/{name}")


if __name__ == "__main__":
    sys.exit(main())
