"""The bundled herdr-core is pinned by checksum and extracted by name."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("fetch", ROOT / "scripts/release/fetch-herdr-core.py")
fetch = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fetch)


def tar_gz(members):
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode="w:gz") as archive:
        for name, body in members.items():
            info = tarfile.TarInfo(name)
            info.size = len(body)
            archive.addfile(info, io.BytesIO(body))
    return data.getvalue()


class FetchHerdrCoreTests(unittest.TestCase):
    def test_every_pinned_target_has_an_https_url_and_a_sha256(self):
        pinned = json.loads((ROOT / "scripts/release/herdr-core.json").read_text())
        for target, entry in pinned["targets"].items():
            self.assertTrue(entry["url"].startswith("https://"), target)
            self.assertIn(pinned["version"], entry["url"], target)
            self.assertRegex(entry["sha256"], "^[0-9a-f]{64}$", target)

    def test_a_checksum_mismatch_fails(self):
        data = b"archive"
        self.assertEqual(fetch.verified(data, hashlib.sha256(data).hexdigest()), data)
        with self.assertRaises(SystemExit):
            fetch.verified(data, "0" * 64)

    def test_the_binary_is_found_by_name_in_either_archive(self):
        self.assertEqual(fetch.executable(tar_gz({"dir/herdr-core": b"elf", "LICENSE": b"x"}), "herdr-core"), b"elf")
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as archive:
            archive.writestr("herdr-core.exe", b"pe")
        self.assertEqual(fetch.executable(data.getvalue(), "herdr-core.exe"), b"pe")
        with self.assertRaises(SystemExit):
            fetch.executable(tar_gz({"other": b"x"}), "herdr-core")

    def test_an_unknown_target_is_refused(self):
        with self.assertRaises(SystemExit):
            fetch.pinned("windows-aarch64")


class BundledHerdrCoreTests(unittest.TestCase):
    def test_every_release_target_bundles_a_pinned_build(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        targets = json.loads(fetch.PINNED.read_text())["targets"]
        for rust, core in fetch.RUST_TARGETS.items():
            self.assertIn(f"target: {rust}", workflow)
            self.assertIn(core, targets)
            self.assertEqual(fetch.pinned(rust), targets[core])

    def test_the_bundled_licenses_are_the_pinned_version_s(self):
        version = json.loads(fetch.PINNED.read_text())["version"]
        licenses = (ROOT / "scripts/release/herdr-core-licenses.txt").read_text()
        self.assertTrue(licenses.startswith(f"herdr-core {version} "), licenses[:80])
        self.assertIn("Apache-2.0", licenses)


if __name__ == "__main__":
    unittest.main()
