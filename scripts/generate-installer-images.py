#!/usr/bin/env python3
"""Generate the Inno Setup installer's page images from the tile artwork.

Renders `sheprd-icon.svg`, the artwork `generate-windows-icons.py`
builds Sheprd.ico from, at each size with `rsvg-convert` (`brew install librsvg`)
or `resvg` (`cargo install resvg`), so the installer matches the app's icon.
The outputs are checked in, so ordinary builds need no renderer.
"""
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile

ASSETS = Path(__file__).resolve().parents[1] / "assets/icons"
SOURCE = ASSETS / "sheprd-icon.svg"
OUTPUT = ASSETS / "windows"
# Inno Setup picks the file nearest the DPI scale: the small image is 55px at
# 100%, and the large one is centered, unstretched, on the welcome page.
WIZARD_SMALL = (55, 69, 83, 110, 138)
WIZARD_LARGE = (128, 160, 192, 256)
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def renderer():
    for name, arguments in (
        ("rsvg-convert", lambda size, source, output: ["--width", str(size), "--height", str(size),
                                                       "--output", str(output), str(source)]),
        ("resvg", lambda size, source, output: ["-w", str(size), "-h", str(size),
                                                str(source), str(output)]),
    ):
        if path := shutil.which(name):
            return lambda size, source, output: subprocess.run(
                [path, *arguments(size, source, output)], check=True)
    raise ValueError("Install rsvg-convert (librsvg) or resvg to rasterize the images")


def png_size(data):
    if data[:8] != PNG_SIGNATURE or data[12:16] != b"IHDR":
        raise ValueError("Rasterizer did not write a PNG")
    return struct.unpack(">II", data[16:24])


def main():
    render = renderer()
    OUTPUT.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory() as temp:
        for prefix, sizes in (("wizard-small", WIZARD_SMALL), ("wizard", WIZARD_LARGE)):
            for size in sizes:
                raster = Path(temp) / f"{size}.png"
                render(size, SOURCE, raster)
                data = raster.read_bytes()
                if png_size(data) != (size, size):
                    raise ValueError(f"Expected a {size}px PNG")
                name = f"{prefix}-{size}.png"
                (OUTPUT / name).write_bytes(data)
                print(f"Generated assets/icons/windows/{name}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
