#!/usr/bin/env python3
"""Repackage the manual Linux archive as .deb, .rpm, and Arch packages.

The packages install the files of Sheprd-VERSION-TARGET.tar.gz under /usr,
materializing its main command alias for the deployed privileged updater.
Every Linux format ships the same bytes. nfpm (pinned by
install-nfpm.sh) writes each format from one generated configuration.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

MAINTAINER = "Fabien Penso <fabienpenso@gmail.com>"
ARCHES = {"x86_64-unknown-linux-gnu": "amd64", "aarch64-unknown-linux-gnu": "arm64"}
# (nfpm packager, published extension)
FORMATS = (("deb", "deb"), ("rpm", "rpm"), ("archlinux", "pkg.tar.zst"))
TREE = {
    "bin/herdr-core", "bin/herdr-gpui", "bin/sheprd",
    *(f"bin/{prefix}-{helper}" for prefix in ("sheprd", "herdr")
      for helper in ("acp", "automate", "nearby", "remote-control")),
    "share/applications/so.pen.herdr-gpui.desktop",
    "share/icons/hicolor/scalable/apps/sheprd.svg",
    *(f"share/licenses/herdr-gpui/{name}" for name in (
        "LICENSE", "NOTICE", "LICENSE-octicons", "LICENSE-APACHE", "NOTICE.md",
        "SOUND-NOTICE.md", "THIRD-PARTY-NOTICES.txt", "HERDR-CORE-LICENSES.txt")),
}
# The release binary is built on Ubuntu 24.04 and requires GLIBC_2.39. It links
# ALSA, FreeType, Fontconfig, xcb and xkbcommon, and dlopens the Vulkan loader and
# libwayland-client, so those are hard requirements too. A Vulkan driver is
# hardware-specific and therefore only recommended. Clipboard acquisition runs
# wl-paste or xclip in the background rather than blocking the UI event loop.
DEPENDS = {
    "deb": ["libc6 (>= 2.39)", "libgcc-s1", "libasound2t64 | libasound2", "libfreetype6", "libfontconfig1",
            "libxcb1", "libxkbcommon0", "libxkbcommon-x11-0", "libwayland-client0", "libvulkan1",
            "wl-clipboard", "xclip"],
    # Soname requirements resolve on any RPM distribution, not only Fedora.
    "rpm": ["libc.so.6(GLIBC_2.39)(64bit)", *(f"{soname}()(64bit)" for soname in (
        "libgcc_s.so.1", "libasound.so.2", "libfreetype.so.6", "libfontconfig.so.1", "libxcb.so.1",
        "libxkbcommon.so.0", "libxkbcommon-x11.so.0", "libwayland-client.so.0",
        "libvulkan.so.1")), "wl-clipboard", "xclip"],
    "archlinux": ["glibc>=2.39", "gcc-libs", "alsa-lib", "freetype2", "fontconfig", "libxcb", "libxkbcommon",
                  "libxkbcommon-x11", "wayland", "vulkan-icd-loader", "wl-clipboard", "xclip"],
}
RECOMMENDS = {
    "deb": ["mesa-vulkan-drivers | vulkan-icd", "fonts-dejavu-core"],
    "rpm": ["mesa-vulkan-drivers"],
    # Arch splits Mesa's Vulkan drivers by vendor; nfpm writes these as optdepends.
    "archlinux": ["vulkan-radeon", "vulkan-intel"],
}


def check_version(version):
    if not re.fullmatch(r"[1-9][0-9]{7}\.[1-9][0-9]*", version):
        raise ValueError("Version must be YYYYMMDD.COUNTER (no v prefix, counter from 1, no leading zeros)")
    # Stamp entries with the release date so rebuilding one release is repeatable.
    return datetime.strptime(version[:8], "%Y%m%d").replace(tzinfo=timezone.utc)


def extract(archive, prefix, destination):
    """Extract regular files and materialize the sole permitted command symlink."""
    with tarfile.open(archive) as source:
        members = source.getmembers()
        files = {}
        seen = set()
        for member in members:
            path = PurePosixPath(member.name)
            if path.parts[:1] != (prefix,) or ".." in path.parts or path.is_absolute():
                raise ValueError(f"Unexpected archive entry: {member.name}")
            relative = str(path.relative_to(prefix))
            if relative in seen:
                raise ValueError(f"Duplicate archive entry: {member.name}")
            seen.add(relative)
            if member.isfile() or (member.issym() and relative == "bin/herdr-gpui"
                                   and member.linkname == "sheprd"):
                files[relative] = member
            elif not member.isdir():
                raise ValueError(f"Archive entry is not a regular file or directory: {member.name}")
        if set(files) != TREE:
            raise ValueError("Archive does not contain the expected Linux tree")
        # Never extract a link. The validated canonical entry must be regular;
        # copy its bytes only after the entire archive has passed validation.
        source.extractall(destination, members=[m for m in files.values() if m.isfile()], filter="data")
    root = destination / prefix
    if files["bin/herdr-gpui"].issym():
        shutil.copy2(root / "bin/sheprd", root / "bin/herdr-gpui")
    return root


def config(version, target, root, mtime):
    contents = [{
        "src": str(root / relative),
        "dst": f"/usr/{relative}",
        "type": "file",
        "file_info": {"mode": 0o755 if relative.startswith("bin/") else 0o644,
                      "mtime": mtime, "owner": "root", "group": "root"},
    } for relative in sorted(TREE)]
    return {
        # Deployed package ownership and the privileged updater pin this name.
        "name": "herdr-gpui",
        "arch": ARCHES[target],
        "platform": "linux",
        "version": version,
        "version_schema": "none",
        "release": "1",
        "mtime": mtime,
        "section": "devel",
        "priority": "optional",
        "maintainer": MAINTAINER,
        "vendor": "Sheprd",
        "homepage": "https://sheprd.org",
        "license": "Apache-2.0",
        "description": "Native GPUI client for an existing local Herdr daemon.\n"
                       "Herdr itself is not included and must be installed separately.",
        "contents": contents,
        # Keep the build machine's hostname out of the published RPM header.
        "rpm": {"buildhost": "herdr-gpui-release"},
        "archlinux": {"packager": MAINTAINER},
        "overrides": {packager: {"depends": DEPENDS[packager], "recommends": RECOMMENDS[packager]}
                      for packager, _ in FORMATS},
    }


def package(version, target, archive, output_dir, nfpm):
    mtime = check_version(version).isoformat().replace("+00:00", "Z")
    if target not in ARCHES:
        raise ValueError("Unsupported Linux target")
    name = f"Sheprd-{version}-{target}"
    if archive.name != f"{name}.tar.gz" or not archive.is_file():
        raise ValueError(f"Expected the manual archive {name}.tar.gz")
    if not output_dir.is_dir():
        raise ValueError("Existing output directory required")
    outputs = [output_dir.resolve() / f"{name}.{extension}" for _, extension in FORMATS]
    # The deployed bridge selects these exact filenames before downloading.
    aliases = [output.with_name(output.name.replace("Sheprd-", "Herdr-", 1)) for output in outputs]
    for output in outputs + aliases:
        if output.exists() or output.is_symlink():
            raise ValueError(f"Output already exists: {output}")
    with tempfile.TemporaryDirectory(dir=output_dir, prefix=".herdr-distro.") as temp:
        temp = Path(temp)
        root = extract(archive, name, temp / "tree")
        (temp / "nfpm.json").write_text(json.dumps(config(version, target, root, mtime), indent=2))
        # Build every format before publishing any, so a failure leaves no partial set.
        built = []
        for (packager, extension), output in zip(FORMATS, outputs):
            partial = temp / f"package.{extension}"
            subprocess.run([str(nfpm), "package", "--config", str(temp / "nfpm.json"),
                            "--packager", packager, "--target", str(partial)],
                           check=True, stdout=subprocess.DEVNULL)
            built.append((partial, output))
        # Copy the completed packages, never rebuild them under the legacy name.
        for (partial, _), alias in zip(built.copy(), aliases):
            copy = temp / alias.name
            shutil.copyfile(partial, copy)
            built.append((copy, alias))
        for partial, output in built:
            if output.exists() or output.is_symlink():
                raise ValueError(f"Output already exists: {output}")
            partial.replace(output)
    return outputs + aliases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument("target")
    parser.add_argument("archive", type=Path)
    parser.add_argument("output_dir", type=Path)
    parser.add_argument("--nfpm", type=Path, default=Path("nfpm"))
    args = parser.parse_args()
    for output in package(args.version, args.target, args.archive, args.output_dir, args.nfpm):
        print(output)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
