"""The host build manifest stays separate from the updater's families."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("update_manifest", Path(__file__).with_name("update-manifest.py"))
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)


class HostManifest(unittest.TestCase):
    def test_both_host_builds_are_required_and_hashed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = []
            for target in updater.HOST_TARGETS:
                with self.assertRaises(ValueError):
                    updater.create(root, "20261017.1", hosts=True)
                self.assertFalse((root / "host-manifest.json").exists())
                name = f"herdr-remote-control-20261017.1-{target}"
                data = name.encode()
                (root / name).write_bytes(data)
                expected.append(dict(target=target, name=name, size=len(data), sha256=hashlib.sha256(data).hexdigest()))
            payload = updater.create(root, "20261017.1", hosts=True)
            self.assertEqual(json.loads(payload), dict(schema=4, version="20261017.1", assets=expected))
            self.assertEqual((root / "host-manifest.json").read_bytes(), payload)
            for other in ("update-manifest.json", "package-manifest.json", "windows-manifest.json"):
                self.assertFalse((root / other).exists())

    def test_host_builds_share_a_manifest_with_no_other_family(self):
        with tempfile.TemporaryDirectory() as directory:
            for family in (dict(packages=True), dict(windows=True)):
                with self.assertRaises(ValueError):
                    updater.create(directory, "20261017.1", hosts=True, **family)

    def test_host_builds_refuse_empty_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for target in updater.HOST_TARGETS:
                (root / f"herdr-remote-control-20261017.1-{target}").touch()
            with self.assertRaises(ValueError):
                updater.create(root, "20261017.1", hosts=True)


if __name__ == "__main__":
    unittest.main()
