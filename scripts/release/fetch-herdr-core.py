"""Download the pinned herdr-core release for one target and verify it.

    python3 scripts/release/fetch-herdr-core.py macos-aarch64 target/herdr-core
    python3 scripts/release/fetch-herdr-core.py aarch64-apple-darwin target/aarch64-apple-darwin/release

Every release bundles this binary beside the app's executable, as a companion
like herdr-acp, so Herdr 2 terminals work with nothing else installed. The
target is herdr-core's own name for it or the Rust target the app was built
for. The version and checksums live in herdr-core.json beside this script; a
mismatch fails instead of bundling the download. Its licenses, as the pinned
binary prints them, are in herdr-core-licenses.txt beside it.
"""
import hashlib
import io
import json
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

PINNED = Path(__file__).with_name("herdr-core.json")
MAX_ARCHIVE = 64 << 20

# The herdr-core build each Rust target the app ships for bundles. herdr-core
# publishes no Windows ARM64 build; Windows 11 on ARM runs the x64 one.
RUST_TARGETS = {
    "aarch64-apple-darwin": "macos-aarch64",
    "x86_64-apple-darwin": "macos-x86_64",
    "aarch64-unknown-linux-gnu": "linux-aarch64",
    "x86_64-unknown-linux-gnu": "linux-x86_64",
    "aarch64-pc-windows-msvc": "windows-x86_64",
    "x86_64-pc-windows-msvc": "windows-x86_64",
}


def pinned(target):
    target = RUST_TARGETS.get(target, target)
    targets = json.loads(PINNED.read_text())["targets"]
    if target not in targets:
        raise SystemExit(f"herdr-core has no {target} build; known: {', '.join(sorted(targets))}")
    return targets[target]


def download(url):
    # The release service refuses urllib's default agent.
    request = urllib.request.Request(url, headers={"User-Agent": "herdr-gpui-release"})
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read(MAX_ARCHIVE + 1)
    if len(data) > MAX_ARCHIVE:
        raise SystemExit(f"{url} is larger than {MAX_ARCHIVE} bytes")
    return data


def verified(data, sha256):
    actual = hashlib.sha256(data).hexdigest()
    if actual != sha256:
        raise SystemExit(f"herdr-core checksum mismatch: expected {sha256}, got {actual}")
    return data


def executable(data, name):
    """The one archive member named `name`, at any depth."""
    if data[:2] == b"PK":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            members = [m for m in archive.namelist() if Path(m).name == name]
            if len(members) == 1:
                return archive.read(members[0])
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            members = [m for m in archive.getmembers() if m.isfile() and Path(m.name).name == name]
            if len(members) == 1:
                return archive.extractfile(members[0]).read()
    raise SystemExit(f"the herdr-core archive has no single {name}")


def main(argv):
    if len(argv) != 3:
        raise SystemExit(__doc__)
    target, out_dir = RUST_TARGETS.get(argv[1], argv[1]), Path(argv[2])
    entry = pinned(target)
    name = "herdr-core.exe" if target.startswith("windows-") else "herdr-core"
    binary = executable(verified(download(entry["url"]), entry["sha256"]), name)
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / name
    partial = path.with_suffix(".partial")
    partial.write_bytes(binary)
    partial.chmod(0o755)
    partial.replace(path)
    print(path)


if __name__ == "__main__":
    main(sys.argv)
