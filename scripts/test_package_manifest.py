"""Package manifest generation stays separate from the legacy updater contract."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("update_manifest", Path(__file__).with_name("update-manifest.py"))
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)


class PackageManifest(unittest.TestCase):
    def test_all_six_packages_are_required_and_hashed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = []
            for target in updater.LINUX_TARGETS:
                for extension in ("deb", "rpm", "pkg.tar.zst"):
                    with self.assertRaises(ValueError):
                        updater.create(root, "20261010.1", packages=True)
                    self.assertFalse((root / "package-manifest.json").exists())
                    name = f"Herdr-20261010.1-{target}.{extension}"
                    data = name.encode()
                    (root / name).write_bytes(data)
                    expected.append(dict(target=target, name=name, size=len(data), sha256=hashlib.sha256(data).hexdigest()))
            payload = updater.create(root, "20261010.1", packages=True)
            self.assertEqual(json.loads(payload), dict(schema=2, version="20261010.1", assets=expected))
            self.assertFalse((root / "update-manifest.json").exists())
            self.assertEqual((root / "package-manifest.json").read_bytes(), payload)

    def test_packages_refuse_symlinks_and_empty_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "Herdr-20261010.1-x86_64-unknown-linux-gnu.deb"
            path.touch()
            with self.assertRaises(ValueError):
                updater.create(root, "20261010.1", packages=True)
            path.unlink()
            target = root / "target"
            target.write_bytes(b"package")
            path.symlink_to(target)
            with self.assertRaises(ValueError):
                updater.create(root, "20261010.1", packages=True)


if __name__ == "__main__":
    unittest.main()
