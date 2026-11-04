"""The Windows installer manifest stays separate from the other updater contracts."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("update_manifest", Path(__file__).with_name("update-manifest.py"))
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)


class WindowsManifest(unittest.TestCase):
    def test_both_installers_are_required_and_hashed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = []
            for target in updater.WINDOWS_TARGETS:
                with self.assertRaises(ValueError):
                    updater.create(root, "20261017.1", windows=True)
                self.assertFalse((root / "windows-manifest.json").exists())
                name = f"Sheprd-20261017.1-{target}-setup.exe"
                data = name.encode()
                (root / name).write_bytes(data)
                expected.append(dict(target=target, name=name, size=len(data), sha256=hashlib.sha256(data).hexdigest()))
            payload = updater.create(root, "20261017.1", windows=True)
            self.assertEqual(json.loads(payload), dict(schema=3, version="20261017.1", assets=expected))
            self.assertFalse((root / "update-manifest.json").exists())
            self.assertFalse((root / "package-manifest.json").exists())
            self.assertEqual((root / "windows-manifest.json").read_bytes(), payload)

    def test_installers_and_packages_never_share_a_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                updater.create(directory, "20261017.1", packages=True, windows=True)

    def test_installers_refuse_empty_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for target in updater.WINDOWS_TARGETS:
                (root / f"Sheprd-20261017.1-{target}-setup.exe").touch()
            with self.assertRaises(ValueError):
                updater.create(root, "20261017.1", windows=True)


if __name__ == "__main__":
    unittest.main()
