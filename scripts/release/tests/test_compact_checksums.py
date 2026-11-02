import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "artifact-manifest.py"
SPEC = importlib.util.spec_from_file_location("artifact_manifest", SCRIPT)
MANIFEST = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MANIFEST)
VERSION = "20261102.2"


class CompactChecksumsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)

    def fixture(self, version=VERSION):
        for name in MANIFEST.base_names(version):
            data = b"s" * 64 if name in MANIFEST.UPDATER_SIGNATURES else name.encode()
            (self.directory / name).write_bytes(data)
            for algorithm in ("sha256", "sha512"):
                digest = hashlib.new(algorithm, data).hexdigest()
                (self.directory / f"{name}.{algorithm}").write_text(f"{digest}  {name}\n")
            for suffix in ("sig", "crt"):
                (self.directory / f"{name}.{suffix}").write_text("signature fixture\n")

    def run_manifest(self, mode, version=VERSION, success=True):
        result = subprocess.run(
            ["python3", str(SCRIPT), mode, version, str(self.directory)],
            capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode == 0, success, result.stderr)
        return result.stdout

    def test_new_release_fits_installed_updater_and_preserves_signed_assets(self):
        self.fixture()
        bases = MANIFEST.base_names(VERSION)
        signed = {name + suffix: (self.directory / (name + suffix)).read_bytes()
                  for name in bases for suffix in ("", ".sig", ".crt")}
        self.run_manifest("create")
        self.run_manifest("verify")
        names = self.run_manifest("names").splitlines()
        self.assertLess(len(names), 128, "older apps reject larger releases")
        self.assertEqual(set(names), {path.name for path in self.directory.iterdir()})
        self.assertFalse(any(name.endswith(".sha512") for name in names))
        sha512 = (self.directory / "SHA512SUMS").read_text()
        self.assertEqual(sha512, "".join(
            f"{hashlib.sha512(signed[name]).hexdigest()}  {name}\n" for name in bases))
        self.assertIn("  SHA512SUMS\n", (self.directory / "SHA256SUMS").read_text())
        for name, data in signed.items():
            self.assertEqual((self.directory / name).read_bytes(), data)

    def test_published_legacy_release_keeps_its_exact_contract(self):
        version = "20261102.1"
        self.fixture(version)
        self.run_manifest("create", version)
        self.run_manifest("verify", version)
        names = self.run_manifest("names", version).splitlines()
        self.assertNotIn("SHA512SUMS", names)
        for name in MANIFEST.base_names(version):
            self.assertIn(name + ".sha512", names)

    def test_bad_signer_checksum_is_rejected_before_consolidating(self):
        self.fixture()
        sidecar = self.directory / (MANIFEST.base_names(VERSION)[-1] + ".sha512")
        sidecar.write_text("incorrect\n")
        before = {path.name: path.read_bytes() for path in self.directory.iterdir()}
        self.run_manifest("create", success=False)
        self.assertEqual(before, {path.name: path.read_bytes() for path in self.directory.iterdir()})

    def test_consolidated_checksum_corruption_or_missing_entries_is_rejected(self):
        self.fixture()
        self.run_manifest("create")
        path = self.directory / "SHA512SUMS"
        original = path.read_text()
        for changed in ("incorrect\n", "".join(original.splitlines(keepends=True)[1:]),
                        original + original.splitlines(keepends=True)[0]):
            with self.subTest(changed=changed[:20]):
                path.write_text(changed)
                self.run_manifest("verify", success=False)

    def test_every_published_asset_is_required_and_extras_are_refused(self):
        self.fixture()
        self.run_manifest("create")
        for name in MANIFEST.asset_names(VERSION):
            with self.subTest(name=name):
                path = self.directory / name
                data = path.read_bytes()
                path.unlink()
                self.run_manifest("verify", success=False)
                path.write_bytes(data)
        (self.directory / "extra.sha512").write_text("extra\n")
        self.run_manifest("verify", success=False)


if __name__ == "__main__":
    unittest.main()
