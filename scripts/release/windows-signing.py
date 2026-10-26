#!/usr/bin/env python3
"""Authenticode-sign the Windows release zip's executables in the release workflow.

  unpack ZIP DIRECTORY
      Extract the tested, unsigned release zip that package-windows.py wrote
      into DIRECTORY, which must not exist, and print the tree root holding
      herdr-gpui.exe, herdr-gpui-launcher.exe, herdr-acp.exe,
      herdr-remote-control.exe, and herdr-automate.exe for the signing action.
  repack ZIP TREE_DIRECTORY OUTPUT_DIR
      Write OUTPUT_DIR/ZIP's name from the signed tree. Refuses unless every
      file is the zip's own and each executable differs from the tested one
      only by its appended Authenticode signature, so signing cannot change
      what was built and tested.
  verify [--subject SUBJECT] ZIP INSTALLER
      On Windows: require a valid, RFC 3161 timestamped signature from one
      signer, optionally exactly SUBJECT, on the zip's executables and on the
      installer.

Signing comes after package-windows.py writes the zip from the tested
executables, so the signed files are exactly what CI built and tested. Pure
standard library, like package-windows.py.
"""
import argparse
import importlib.util
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
EXECUTABLES = ("herdr-gpui.exe", "herdr-gpui-launcher.exe", "herdr-acp.exe",
               "herdr-remote-control.exe", "herdr-automate.exe")
# IMAGE_OPTIONAL_HEADER offsets: CheckSum, NumberOfRvaAndSizes, and the
# certificate table, data directory 4, whose address is a file offset.
CHECKSUM_OFFSET = 64
DIRECTORIES = {0x10B: (92, 96), 0x20B: (108, 112)}
SECURITY_DIRECTORY = 4
# WIN_CERTIFICATE entries start 8-byte aligned, so signing may pad first.
CERTIFICATE_ALIGNMENT = 8


def load_package():
    spec = importlib.util.spec_from_file_location("package_windows", ROOT / "scripts/release/package-windows.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def certificate_table(data):
    """(offset of the CheckSum field, offset of the certificate table entry,
    certificate table file offset, size) of PE image `data`."""
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise ValueError("Binary is not a Windows executable")
    optional = struct.unpack_from("<I", data, 0x3C)[0] + 24
    if data[optional - 24:optional - 20] != b"PE\0\0" or len(data) < optional + 2:
        raise ValueError("Binary is not a Windows executable")
    magic = struct.unpack_from("<H", data, optional)[0]
    if magic not in DIRECTORIES:
        raise ValueError("Binary has no PE32 or PE32+ optional header")
    count_offset, directories = DIRECTORIES[magic]
    entry = optional + directories + SECURITY_DIRECTORY * 8
    if len(data) < entry + 8 or struct.unpack_from("<I", data, optional + count_offset)[0] <= SECURITY_DIRECTORY:
        raise ValueError("Binary has no certificate table directory")
    offset, size = struct.unpack_from("<II", data, entry)
    return optional + CHECKSUM_OFFSET, entry, offset, size


def without_signature(signed, unsigned):
    """Refuse unless `signed` is `unsigned` plus one appended Authenticode
    signature: the only other changes signing makes are the checksum and the
    certificate table entry, and alignment padding before the table."""
    checksum, entry, offset, size = certificate_table(unsigned)
    if (offset, size) != (0, 0):
        raise ValueError("Tested executable is already signed")
    if certificate_table(signed)[:2] != (checksum, entry):
        raise ValueError("Signing changed the PE headers")
    offset, size = certificate_table(signed)[2:]
    if not size or offset + size != len(signed) or offset % CERTIFICATE_ALIGNMENT:
        raise ValueError("Executable has no appended Authenticode signature")
    padding = signed[len(unsigned):offset]
    if not 0 <= offset - len(unsigned) < CERTIFICATE_ALIGNMENT or padding.strip(b"\0"):
        raise ValueError("Signing changed the executable's contents")
    stripped = bytearray(signed[:len(unsigned)])
    original = bytearray(unsigned)
    for image in (stripped, original):
        struct.pack_into("<I", image, checksum, 0)
        struct.pack_into("<II", image, entry, 0, 0)
    if stripped != original:
        raise ValueError("Signing changed the executable's contents")


def unpack(archive, directory):
    if directory.exists() or directory.is_symlink():
        raise ValueError(f"Signing directory already exists: {directory}")
    root = archive.name.removesuffix(".zip")
    with zipfile.ZipFile(archive) as release:
        names = release.namelist()
        if any(not name.startswith(root + "/") or ".." in name.split("/") for name in names):
            raise ValueError("Release zip has entries outside its root")
        for required in EXECUTABLES:
            if f"{root}/{required}" not in names:
                raise ValueError(f"Release zip has no {required}")
        directory.mkdir(parents=True)
        release.extractall(directory)
    return directory.resolve() / root


def repack(archive, tree, output_dir):
    if not output_dir.is_dir():
        raise ValueError("Existing output directory required")
    output = output_dir.resolve() / archive.name
    if output.exists() or output.is_symlink():
        raise ValueError(f"Output already exists: {output}")
    root = archive.name.removesuffix(".zip")
    with zipfile.ZipFile(archive) as release:
        names = [name for name in release.namelist() if not name.endswith("/")]
        files = {path.relative_to(tree).as_posix() for path in (tree / root).rglob("*") if not path.is_dir()}
        if files != set(names):
            raise ValueError("Signed tree does not hold exactly the release zip's files")
        contents = {}
        for name in names:
            data = (tree / name).read_bytes()
            if name.removeprefix(root + "/") in EXECUTABLES:
                without_signature(data, release.read(name))
            elif data != release.read(name):
                raise ValueError(f"Signing changed {name}")
            contents[name] = data
    package = load_package()
    # Build beside the output and rename, as package-windows.py does, in the
    # zip's own entry order and with its fixed timestamps.
    with tempfile.NamedTemporaryFile(dir=output.parent, prefix=".herdr-windows.", delete=False) as temp:
        partial = Path(temp.name)
    try:
        with zipfile.ZipFile(partial, "w") as signed:
            for name in names:
                package.add(signed, name, contents[name])
        partial.replace(output)
    finally:
        partial.unlink(missing_ok=True)
    return output


def signatures(paths):
    """(status, timestamped, signer subject) for each of `paths`, from Windows itself."""
    script = ("$ErrorActionPreference = 'Stop'; "
              "foreach ($path in $env:HERDR_SIGNED_FILES -split '\\|') { "
              "$s = Get-AuthenticodeSignature -LiteralPath $path; "
              "'{0}|{1}|{2}' -f $s.Status, ($null -ne $s.TimeStamperCertificate), $s.SignerCertificate.Subject }")
    result = subprocess.run(["pwsh", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
                            capture_output=True, text=True, check=True, timeout=120,
                            env={**os.environ, "HERDR_SIGNED_FILES": "|".join(map(str, paths))})
    return [tuple(line.split("|", 2)) for line in result.stdout.splitlines() if line]


def check_signed(paths, subject=None):
    results = signatures(paths)
    if len(results) != len(paths):
        raise ValueError("Could not read every Authenticode signature")
    for path, (status, timestamped, signer) in zip(paths, results):
        if status != "Valid" or timestamped != "True":
            raise ValueError(f"{path} is not validly signed with a timestamp: {status}")
        if subject is not None and signer != subject:
            raise ValueError(f"{path} is signed by {signer!r}, not {subject!r}")
    if len({signer for _, _, signer in results}) != 1:
        raise ValueError("Files are signed by different signers")


def verify(archive, installer, subject):
    with tempfile.TemporaryDirectory(prefix="herdr-signed-") as temp:
        tree = unpack(archive, Path(temp) / "tree")
        check_signed([tree / name for name in EXECUTABLES] + [installer.resolve()], subject)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    extract = commands.add_parser("unpack")
    extract.add_argument("archive", type=Path)
    extract.add_argument("directory", type=Path)
    pack = commands.add_parser("repack")
    pack.add_argument("archive", type=Path)
    pack.add_argument("tree", type=Path)
    pack.add_argument("output_dir", type=Path)
    check = commands.add_parser("verify")
    check.add_argument("--subject")
    check.add_argument("archive", type=Path)
    check.add_argument("installer", type=Path)
    args = parser.parse_args()
    if args.command == "unpack":
        print(unpack(args.archive, args.directory))
    elif args.command == "repack":
        print(repack(args.archive, args.tree, args.output_dir))
    else:
        verify(args.archive, args.installer, args.subject)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
