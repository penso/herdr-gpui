#!/usr/bin/env python3
"""Submit a published stable release's Windows installers to winget.

  install-komac DESTINATION_DIR
      Install the pinned Komac release for Linux x86_64 into DESTINATION_DIR
      after checking its SHA-256, and print the path of the executable.
  submit VERSION SHA256SUMS KOMAC OUTPUT_DIR
      With GITHUB_TOKEN set to a classic token with the public_repo scope:
      have Komac write the manifests for the published installers into
      OUTPUT_DIR from the package's previous winget-pkgs version, refuse
      unless they describe exactly those installers, then open the pull
      request to microsoft/winget-pkgs. Warns and does nothing while the
      package has no version in winget-pkgs yet: the first submission is the
      maintainer's (scripts/release/README.md).
  check VERSION SHA256SUMS MANIFEST_DIR
      The manifest check alone, over Komac's OUTPUT_DIR.

Komac carries the publisher, name, scope and descriptions forward from the
previous version; submit refreshes the visible name and project URLs. IDENTIFIER and
PUBLISHER must never change: winget matches installed apps and upgrades by
them. Pure standard library.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
IDENTIFIER = "Penso.HerdrUI"
# AppPublisher and AppId in sheprd.iss; Inno's uninstall key appends _is1.
PUBLISHER = "Herdr UI"
PRODUCT_CODE = "{7FA70894-4FC0-4923-BDCB-B05F7475610A}_is1"
RELEASES = "https://github.com/sheprd-labs/sheprd/releases"
# winget's architecture for each release target; arm64 is listed when its
# installer is part of the release.
ARCHITECTURES = {"x86_64-pc-windows-msvc": "x64", "aarch64-pc-windows-msvc": "arm64"}
REQUIRED = "x86_64-pc-windows-msvc"
# The digest was taken from the release asset when the pin was chosen; bump
# both together.
KOMAC_VERSION = "2.16.0"
KOMAC_URL = (f"https://github.com/russellbanks/Komac/releases/download/v{KOMAC_VERSION}/"
             f"komac-{KOMAC_VERSION}-x86_64-unknown-linux-gnu.tar.gz")
KOMAC_SHA256 = "7d2707fa6210f2789a3702de49fbd150b736dbf426ee0b9bc8e098736f9fd82d"


def check_version(version):
    if not re.fullmatch(r"[1-9][0-9]{7}\.[1-9][0-9]*", version):
        raise ValueError("Version must be YYYYMMDD.COUNTER (no v prefix, counter from 1, no leading zeros)")


def installers(version, sums):
    """{winget architecture: (installer URL, SHA-256)} for the setup.exe assets
    SHA256SUMS lists, which the publish job verified against the release."""
    check_version(version)
    digests = {}
    for line in sums.read_text().splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9._-]+)", line)
        if not match:
            raise ValueError("Malformed checksum entry")
        digests[match[2]] = match[1]
    found = {}
    for target, architecture in ARCHITECTURES.items():
        name = f"Sheprd-{version}-{target}-setup.exe"
        if name in digests:
            found[architecture] = (f"{RELEASES}/download/v{version}/{name}", digests[name])
        elif target == REQUIRED:
            raise ValueError(f"SHA256SUMS lists no {name}")
    return found


def scalar(value):
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "'\"":
        return value[1:-1]
    return value


def parse(text):
    """(top-level fields, [installer entry fields]) of a manifest as Komac
    writes it. Only the scalar keys this check reads need to parse; nested
    lists and mappings inside an installer are skipped."""
    fields, entries, section = {}, [], None
    for line in text.splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if not line[0].isspace() and not line.startswith("- "):
            key, _, value = line.partition(":")
            section = key
            if value.strip():
                fields[key] = scalar(value)
            continue
        if section != "Installers":
            continue
        if line.startswith("- "):
            entries.append({})
            line = "  " + line[2:]
        if re.match(r"  [A-Za-z]", line) and entries:
            key, _, value = line[2:].partition(":")
            if value.strip():
                entries[-1][key] = scalar(value)
    return fields, entries


def check(version, sums, output):
    expected = installers(version, sums)
    directory = output / "manifests" / IDENTIFIER[0].lower() / Path(*IDENTIFIER.split(".")) / version
    manifests = {path.name: parse(path.read_text()) for path in directory.glob("*.yaml")}
    by_type = {}
    for name, (fields, _) in manifests.items():
        if (fields.get("PackageIdentifier"), fields.get("PackageVersion")) != (IDENTIFIER, version):
            raise ValueError(f"{name} is not {IDENTIFIER} {version}")
        by_type.setdefault(fields.get("ManifestType"), []).append(fields)
    if [len(by_type.get(kind, [])) for kind in ("version", "installer", "defaultLocale")] != [1, 1, 1]:
        raise ValueError(f"{directory} needs one version, installer and default locale manifest")
    # Other locales may leave the publisher to the default locale.
    publishers = [by_type["defaultLocale"][0].get("Publisher")]
    publishers += [fields["Publisher"] for fields in by_type.get("locale", []) if "Publisher" in fields]
    if set(publishers) != {PUBLISHER}:
        raise ValueError(f"Publisher changed to {publishers!r}")
    if by_type["defaultLocale"][0].get("PackageName") != "Sheprd":
        raise ValueError("PackageName must show Sheprd while retaining the deployed publisher")
    fields, entries = manifests[f"{IDENTIFIER}.installer.yaml"]
    found = {}
    for entry in entries:
        effective = {**fields, **entry}
        if (effective.get("InstallerType"), effective.get("Scope")) != ("inno", "user"):
            raise ValueError("Every installer must be a per-user Inno Setup installer")
        if effective.get("ProductCode", PRODUCT_CODE) != PRODUCT_CODE:
            raise ValueError(f"ProductCode changed to {effective['ProductCode']!r}")
        architecture = effective.get("Architecture")
        if architecture in found:
            raise ValueError(f"Two {architecture} installers")
        found[architecture] = (effective.get("InstallerUrl"), effective.get("InstallerSha256", "").lower())
    if found != expected:
        raise ValueError(f"Installers {found} are not the release's {expected}")
    return directory


def install_komac(destination):
    with urllib.request.urlopen(KOMAC_URL, timeout=120) as response:
        archive = response.read()
    actual = hashlib.sha256(archive).hexdigest()
    if actual != KOMAC_SHA256:
        raise ValueError(f"Komac archive digest mismatch: {actual}")
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        member = tar.getmember("komac")
        if not member.isfile():
            raise ValueError("Komac archive has no komac executable")
        data = tar.extractfile(member).read()
    destination.mkdir(parents=True, exist_ok=True)
    komac = destination.resolve() / "komac"
    komac.write_bytes(data)
    komac.chmod(0o755)
    return komac


def published():
    """Whether winget-pkgs has a version of the package for Komac to update."""
    path = "/".join(["manifests", IDENTIFIER[0].lower(), *IDENTIFIER.split(".")])
    request = urllib.request.Request(
        f"https://api.github.com/repos/microsoft/winget-pkgs/contents/{path}",
        headers={"Authorization": "Bearer " + os.environ["GITHUB_TOKEN"],
                 "Accept": "application/vnd.github+json"})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            return any(entry["type"] == "dir" for entry in json.load(response))
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise


def submit(version, sums, komac, output):
    expected = installers(version, sums)
    if not os.environ.get("GITHUB_TOKEN"):
        raise ValueError("GITHUB_TOKEN must hold the winget-pkgs submission token")
    if not published():
        print(f"::warning::{IDENTIFIER} is not in winget-pkgs yet; submit its first version by hand "
              "(scripts/release/README.md)")
        return
    if output.exists():
        raise ValueError(f"Output already exists: {output}")
    urls = [expected[architecture][0] for architecture in sorted(expected)]
    common = [str(komac), "update", IDENTIFIER, "--version", version, "--urls", *urls,
              "--release-notes-url", f"{RELEASES}/tag/v{version}"]
    subprocess.run([*common, "--dry-run", "--output", str(output)], check=True, timeout=900)
    # Komac carries the previous locale's display name forward. Update visible
    # branding only; publisher, identifier and product code remain pinned below.
    for manifest in output.rglob("*.yaml"):
        text = manifest.read_text()
        text = re.sub(r"^PackageName: .+$", "PackageName: Sheprd", text, flags=re.M)
        text = text.replace("https://herdrui.dev", "https://sheprd.org")
        text = text.replace("https://github.com/penso/herdr-gpui", "https://github.com/sheprd-labs/sheprd")
        manifest.write_text(text)
    directory = check(version, sums, output)
    subprocess.run([str(komac), "submit", "--yes", str(directory)], check=True, timeout=900)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    install = commands.add_parser("install-komac")
    install.add_argument("destination", type=Path)
    send = commands.add_parser("submit")
    send.add_argument("version")
    send.add_argument("sums", type=Path)
    send.add_argument("komac", type=Path)
    send.add_argument("output", type=Path)
    verify = commands.add_parser("check")
    verify.add_argument("version")
    verify.add_argument("sums", type=Path)
    verify.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.command == "install-komac":
        print(install_komac(args.destination))
    elif args.command == "submit":
        submit(args.version, args.sums, args.komac, args.output)
    else:
        print(check(args.version, args.sums, args.output))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, tarfile.TarError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
