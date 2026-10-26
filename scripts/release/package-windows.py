#!/usr/bin/env python3
"""Package the Windows executable as Herdr-VERSION-TARGET.zip.

Pure standard library so the Windows runner needs no zip tool. The layout
mirrors package-linux.sh: the executable plus the same license and notice files.
Beside herdr-gpui.exe, which is a console program because it is also the CLI,
the zip carries herdr-gpui-launcher.exe, the windowed launcher that starts it without a
console window from Explorer or the Start menu, herdr-acp.exe, which the app
starts chats with, herdr-remote-control.exe, the remote control daemon the
app runs as a scheduled task, and herdr-automate.exe, the automation daemon;
the build leaves all three beside herdr-gpui.exe.
"""
import argparse
from pathlib import Path
import re
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
TARGETS = ("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc")
LICENSES = ("LICENSE", "NOTICE", "assets/icons/LICENSE-octicons",
            "crates/herdr-protocol/LICENSE-APACHE", "crates/herdr-protocol/NOTICE.md",
            "crates/herdr-gpui/SOUND-NOTICE.md")
# Zip cannot store dates before 1980; a fixed stamp keeps entries independent
# of checkout and build times.
TIMESTAMP = (1980, 1, 1, 0, 0, 0)


def add(archive, name, data):
    info = zipfile.ZipInfo(name, TIMESTAMP)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o644 << 16
    archive.writestr(info, data)


def package(version, target, binary, launcher, output_dir, notices):
    if not re.fullmatch(r"[1-9][0-9]{7}\.[1-9][0-9]*", version):
        raise ValueError("Version must be YYYYMMDD.COUNTER (no v prefix, counter from 1, no leading zeros)")
    if target not in TARGETS:
        raise ValueError("Unsupported Windows target")
    if not binary.is_file() or not output_dir.is_dir():
        raise ValueError("Binary file and existing output directory required")
    if not launcher.is_file():
        raise ValueError("Launcher file required")
    companions = [binary.parent / name for name in ("herdr-acp.exe", "herdr-remote-control.exe",
                                                         "herdr-automate.exe")]
    for companion in companions:
        if not companion.is_file():
            raise ValueError(f"A {companion.name} beside the binary required")
    if not notices.is_file() or not notices.stat().st_size:
        raise ValueError("Nonempty third-party notices file required")
    name = f"Herdr-{version}-{target}"
    output = output_dir.resolve() / f"{name}.zip"
    if output.exists() or output.is_symlink():
        raise ValueError(f"Output already exists: {output}")
    # Build beside the output and rename, so a failure never leaves a partial zip.
    with tempfile.NamedTemporaryFile(dir=output.parent, prefix=".herdr-windows.", delete=False) as temp:
        partial = Path(temp.name)
    try:
        with zipfile.ZipFile(partial, "w") as archive:
            add(archive, f"{name}/herdr-gpui.exe", binary.read_bytes())
            add(archive, f"{name}/herdr-gpui-launcher.exe", launcher.read_bytes())
            for companion in companions:
                add(archive, f"{name}/{companion.name}", companion.read_bytes())
            for source in LICENSES:
                add(archive, f"{name}/licenses/{Path(source).name}", (ROOT / source).read_bytes())
            add(archive, f"{name}/licenses/THIRD-PARTY-NOTICES.txt", notices.read_bytes())
        if output.exists() or output.is_symlink():
            raise ValueError(f"Output already exists: {output}")
        partial.replace(output)
    finally:
        partial.unlink(missing_ok=True)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument("target")
    parser.add_argument("binary", type=Path)
    parser.add_argument("launcher", type=Path)
    parser.add_argument("output_dir", type=Path)
    parser.add_argument("notices", type=Path)
    args = parser.parse_args()
    print(package(args.version, args.target, args.binary, args.launcher, args.output_dir, args.notices))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
