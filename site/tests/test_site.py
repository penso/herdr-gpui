"""Static publishing contracts and isolated Linux installer execution."""
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit
import fnmatch
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "site"


class Page(HTMLParser):
    def __init__(self, path):
        super().__init__()
        self.links = []
        self.ids = set()
        self.feed(path.read_text())

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        for key in ("href", "src"):
            if key in attrs:
                self.links.append(attrs[key])
        if attrs.get("property") == "og:image":
            self.links.append(attrs["content"])


class PublishingTests(unittest.TestCase):
    def test_page_links_resolve_and_fragments_exist(self):
        for path in SITE.rglob("*.html"):
            for link in Page(path).links:
                parsed = urlsplit(link)
                if parsed.netloc and parsed.netloc != "sheprd.org":
                    continue
                target = SITE / parsed.path.lstrip("/") if parsed.path.startswith("/") else path.parent / parsed.path
                if not parsed.path:
                    target = path
                if target.is_dir():
                    target /= "index.html"
                with self.subTest(page=path.name, link=link):
                    self.assertTrue(target.is_file(), target)
                    if parsed.fragment:
                        self.assertIn(parsed.fragment, Page(target).ids)

    def test_netlify_only_canonicalizes_www_and_blocks_internal_files(self):
        config = tomllib.loads((ROOT / "netlify.toml").read_text())
        self.assertEqual(config["build"]["publish"], "site")
        self.assertEqual(config["build"]["command"], "")
        redirects = config["redirects"]
        canonical = [r for r in redirects if r["status"] == 301]
        self.assertEqual({r["from"] for r in canonical}, {
            "http://www.sheprd.org/*", "https://www.sheprd.org/*"})
        self.assertTrue(all(r["to"] == "https://sheprd.org/:splat" for r in canonical))
        for path in ("/README.md", "/tests/test_site.py", "/worker/index.js", "/wrangler.jsonc"):
            with self.subTest(path=path):
                self.assertTrue(any(r["status"] == 404 and r.get("force") and
                                    fnmatch.fnmatch(path, r["from"]) for r in redirects))
        headers = (SITE / "_headers").read_text()
        for script in ("install.sh", "install.ps1"):
            self.assertIn(f"/{script}\n  Content-Type: text/plain; charset=utf-8", headers)

    def test_social_image_is_the_finalized_open_graph_size(self):
        image = SITE / "assets/sheprd-social-preview.png"
        data = image.read_bytes()
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
        self.assertEqual(struct.unpack(">II", data[16:24]), (1200, 630))
        self.assertEqual(data, (ROOT / "assets/branding/sheprd-social.png").read_bytes())


MOCK = r'''
from pathlib import Path
import hashlib
import json
import os
import sys

name = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["LOG"], "a") as log:
    log.write(json.dumps([name, *args]) + "\n")
if name == "uname":
    print(os.environ.get("ARCH", "x86_64") if args == ["-m"] else "Linux")
elif name == "id":
    print("0")
elif name == "curl":
    url = args[-1]
    if url.endswith("/latest"):
        print("https://github.com/sheprd-labs/sheprd/releases/tag/v20261103.1", end="")
    else:
        output = Path(args[args.index("-o") + 1])
        if url.endswith(".sha256"):
            package = output.with_suffix("")
            digest = hashlib.sha256(package.read_bytes()).hexdigest()
            if os.environ.get("CORRUPT"):
                digest = "0" * 64
            output.write_text(f"{digest}  {package.name}\n")
        else:
            output.write_bytes(b"isolated package fixture")
elif name == "sha256sum":
    digest, filename = Path(args[-1]).read_text().split()
    sys.exit(hashlib.sha256(Path(filename).read_bytes()).hexdigest() != digest)
elif name in ("apt-get", "dnf", "pacman"):
    assert Path(args[-1]).read_bytes() == b"isolated package fixture"
else:
    raise AssertionError(name)
'''


class LinuxInstallerTests(unittest.TestCase):
    def run_installer(self, manager, arch="x86_64", corrupt=False):
        import json
        with tempfile.TemporaryDirectory(prefix="sheprd-site-install-") as directory:
            base = Path(directory)
            bin_dir = base / "bin"
            bin_dir.mkdir()
            for command in ("uname", "id", "curl", "sha256sum", manager):
                path = bin_dir / command
                path.write_text(f"#!{sys.executable}\n" + MOCK)
                path.chmod(0o755)
            # No real package manager, sudo, daemon, network or user config is
            # reachable through PATH. Only temporary-file utilities are real.
            for command in ("mktemp", "rm", "chmod"):
                (bin_dir / command).symlink_to(shutil.which(command))
            log = base / "log"
            result = subprocess.run(["/bin/sh", str(SITE / "install.sh")],
                                    env={"PATH": str(bin_dir), "LOG": str(log),
                                         "ARCH": arch, "CORRUPT": "1" if corrupt else "",
                                         "TMPDIR": str(base)},
                                    capture_output=True, text=True, timeout=30)
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            return result, calls

    def test_new_release_assets_install_through_each_package_manager(self):
        for manager, suffix in (("apt-get", "deb"), ("dnf", "rpm"), ("pacman", "pkg.tar.zst")):
            for arch in ("x86_64", "aarch64"):
                with self.subTest(manager=manager, arch=arch):
                    result, calls = self.run_installer(manager, arch)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    asset = f"Sheprd-20261103.1-{arch}-unknown-linux-gnu.{suffix}"
                    installs = [c for c in calls if c[0] == manager]
                    self.assertEqual(len(installs), 1)
                    self.assertEqual(Path(installs[0][-1]).name, asset)
                    urls = [c[-1] for c in calls if c[0] == "curl"]
                    self.assertIn(f"https://github.com/sheprd-labs/sheprd/releases/download/v20261103.1/{asset}", urls)
                    self.assertIn("run: sheprd", result.stdout)

    def test_checksum_failure_never_runs_the_package_manager(self):
        result, calls = self.run_installer("apt-get", corrupt=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checksum mismatch", result.stderr)
        self.assertFalse(any(c[0] == "apt-get" for c in calls))


if __name__ == "__main__":
    unittest.main()
