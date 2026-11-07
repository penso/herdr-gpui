#!/usr/bin/env python3
"""One exact release asset contract, shared by publication and consumers."""

import argparse
import hashlib
import pathlib
import re


def base_names(version, legacy=False):
    if not re.fullmatch(r"[1-9][0-9]{7}\.[1-9][0-9]*", version):
        raise ValueError("Expected YYYYMMDD.COUNTER version")
    names = [f"Sheprd-{version}-universal-apple-darwin.dmg",
            f"Sheprd-{version}-x86_64-unknown-linux-gnu.tar.gz",
            f"Sheprd-{version}-x86_64-unknown-linux-gnu.deb",
            f"Sheprd-{version}-x86_64-unknown-linux-gnu.rpm",
            f"Sheprd-{version}-x86_64-unknown-linux-gnu.pkg.tar.zst",
            f"Sheprd-{version}-aarch64-unknown-linux-gnu.tar.gz",
            f"Sheprd-{version}-aarch64-unknown-linux-gnu.deb",
            f"Sheprd-{version}-aarch64-unknown-linux-gnu.rpm",
            f"Sheprd-{version}-aarch64-unknown-linux-gnu.pkg.tar.zst",
            f"Sheprd-{version}-x86_64-pc-windows-msvc.zip",
            f"Sheprd-{version}-aarch64-pc-windows-msvc.zip",
            f"Sheprd-{version}-x86_64-pc-windows-msvc-setup.exe",
            f"Sheprd-{version}-aarch64-pc-windows-msvc-setup.exe",
            f"Sheprd-{version}.cdx.json",
            f"sheprd-{version}-macos-universal.app.tar.gz",
            f"sheprd-{version}-x86_64-unknown-linux-gnu-update.tar.gz",
            f"sheprd-{version}-aarch64-unknown-linux-gnu-update.tar.gz",
            "update-manifest.json", "update-manifest.sig",
            "package-manifest.json", "package-manifest.sig",
            "windows-manifest.json", "windows-manifest.sig",
            f"sheprd-remote-control-{version}-x86_64-unknown-linux-musl",
            f"sheprd-remote-control-{version}-aarch64-unknown-linux-musl",
            "host-manifest.json", "host-manifest.sig"]
    if legacy:
        return [name.replace("Sheprd-", "Herdr-").replace("sheprd-remote-control-", "herdr-remote-control-")
                .replace("sheprd-", "herdr-gpui-") for name in names]
    return names


UPDATER_SIGNATURES = ("update-manifest.sig", "package-manifest.sig", "windows-manifest.sig",
                      "host-manifest.sig")


def package_aliases(version, legacy=False):
    """Bare legacy download names; signatures/provenance cover identical bases."""
    if legacy:
        return {}
    return {name.replace("Sheprd-", "Herdr-", 1): name for name in base_names(version)
            if name.endswith((".deb", ".rpm", ".pkg.tar.zst"))}


def check_aliases(directory, version, legacy=False):
    for alias, original in package_aliases(version, legacy).items():
        with (directory / alias).open("rb") as copy, (directory / original).open("rb") as source:
            while True:
                chunk = source.read(65536)
                if copy.read(65536) != chunk:
                    raise ValueError(f"Package alias differs from signed artifact: {alias}")
                if not chunk:
                    break


def compact_checksums(version):
    # Published releases are immutable. Keep their exact old asset contract
    # verifiable while fitting new releases under installed updaters' 128 cap.
    return tuple(map(int, version.split("."))) >= (20261102, 2)


def asset_names(version, legacy=False):
    bases = base_names(version, legacy)
    suffixes = ("", ".sha256", ".sig", ".crt") if compact_checksums(version) else (
        "", ".sha256", ".sha512", ".sig", ".crt")
    names = [name + suffix for name in bases for suffix in suffixes]
    names.extend(package_aliases(version, legacy))
    if compact_checksums(version):
        names.append("SHA512SUMS")
    return sorted(names)


def check_files(directory, names):
    files = list(directory.iterdir())
    if {p.name for p in files} != set(names) or any(
        p.is_symlink() or not p.is_file() or not p.stat().st_size for p in files
    ):
        raise ValueError("Missing, empty, symlink, or unexpected release artifact")
    for signature in UPDATER_SIGNATURES:
        if signature in names and (directory / signature).stat().st_size != 64:
            raise ValueError("Updater signature must be raw 64-byte Ed25519")


def digest(path, algorithm):
    with path.open("rb") as source:
        return hashlib.file_digest(source, algorithm).hexdigest()


def consolidate_checksums(directory, version, legacy=False):
    """Validate the signer's full output before replacing SHA-512 sidecars."""
    bases = base_names(version, legacy)
    aliases = package_aliases(version, legacy)
    check_files(directory, [name + suffix for name in bases
                            for suffix in ("", ".sha256", ".sha512", ".sig", ".crt")] + list(aliases))
    check_aliases(directory, version, legacy)
    lines = []
    for name in bases:
        expected = f"{digest(directory / name, 'sha512')}  {name}\n"
        if (directory / f"{name}.sha512").read_text() != expected:
            raise ValueError(f"Invalid sha512 sidecar: {name}")
        lines.append(expected)
    lines.extend(f"{digest(directory / name, 'sha512')}  {name}\n" for name in aliases)
    with (directory / "SHA512SUMS").open("x") as output:
        output.write("".join(lines))
    for name in bases:
        (directory / f"{name}.sha512").unlink()


def verify_manifest(directory, version, legacy=False):
    entries = {}
    for line in (directory / "SHA256SUMS").read_text().splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9._-]+)", line)
        if not match or match[2] in entries:
            raise ValueError("Malformed or duplicate checksum entry")
        entries[match[2]] = match[1]
    if set(entries) != set(asset_names(version, legacy)):
        raise ValueError("Unexpected checksum manifest")
    return entries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("base", "create", "verify", "dmg", "names", "base-names"))
    parser.add_argument("version")
    parser.add_argument("directory", type=pathlib.Path)
    parser.add_argument("--legacy-names", action="store_true",
                        help="use the exact pre-rebrand asset set for historical verification")
    args = parser.parse_args()
    directory, version = args.directory, args.version
    names = asset_names(version, args.legacy_names)
    bases = base_names(version, args.legacy_names)
    if args.mode == "base-names":
        print("\n".join(bases))
        return
    if args.mode == "names":
        print("\n".join(names + ["SHA256SUMS"]))
        return
    if args.mode == "base":
        check_files(directory, bases + list(package_aliases(version, args.legacy_names)))
        check_aliases(directory, version, args.legacy_names)
        return
    if args.mode == "dmg":
        entries = verify_manifest(directory, version, args.legacy_names)
        name = bases[0]
        check_files(directory, [name, "SHA256SUMS"])
        actual = digest(directory / name, "sha256")
        if actual != entries[name]:
            raise ValueError("Final DMG checksum mismatch")
        print(actual)
        return
    if args.mode == "create" and compact_checksums(version):
        if len(names) + 1 >= 128:
            raise ValueError("Release must have fewer than 128 assets for installed updaters")
        consolidate_checksums(directory, version, args.legacy_names)
    check_files(directory, names + ([] if args.mode == "create" else ["SHA256SUMS"]))
    check_aliases(directory, version, args.legacy_names)
    sha512_lines = []
    for name in bases:
        for algorithm in ("sha256", "sha512"):
            expected = f"{digest(directory / name, algorithm)}  {name}\n"
            if algorithm == "sha512" and compact_checksums(version):
                sha512_lines.append(expected)
            elif (directory / f"{name}.{algorithm}").read_text() != expected:
                raise ValueError(f"Invalid {algorithm} sidecar: {name}")
    sha512_lines.extend(f"{digest(directory / name, 'sha512')}  {name}\n"
                        for name in package_aliases(version, args.legacy_names))
    if compact_checksums(version) and (directory / "SHA512SUMS").read_text() != "".join(sha512_lines):
        raise ValueError("Invalid SHA512SUMS manifest")
    if args.mode == "create":
        with (directory / "SHA256SUMS").open("x") as output:
            for name in names:
                output.write(f"{digest(directory / name, 'sha256')}  {name}\n")
    else:
        for name, expected in verify_manifest(directory, version, args.legacy_names).items():
            if digest(directory / name, "sha256") != expected:
                raise ValueError(f"Checksum mismatch: {name}")


if __name__ == "__main__":
    main()
