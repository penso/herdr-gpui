"""The deployed bridge must find one signed Linux package slot per format."""
import hashlib
import importlib.util
import json
import subprocess
import unittest

import test_linux_distro as fixtures
from test_linux_distro import EXTENSIONS, TARGETS, VERSION, ROOT, SCRIPTS


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


MANIFEST = load("artifact_manifest", SCRIPTS / "artifact-manifest.py")
UPDATER = load("update_manifest", ROOT / "scripts/update-manifest.py")


class LinuxBridgePublication(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.LinuxDistroTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)

    def test_packages_have_identical_aliases_and_one_signed_slot_per_target_format(self):
        directory = self.fixture.work / "dist"
        for target in TARGETS:
            archive = self.fixture.archive(target, directory)
            self.fixture.package(target, archive)
            for extension in EXTENSIONS:
                original = directory / f"Sheprd-{VERSION}-{target}.{extension}"
                alias = directory / f"Herdr-{VERSION}-{target}.{extension}"
                self.assertFalse(alias.is_symlink())
                self.assertEqual(alias.read_bytes(), original.read_bytes())
        manifest = json.loads(UPDATER.create(directory, VERSION, packages=True))
        expected = {(target, f"Herdr-{VERSION}-{target}.{extension}")
                    for target in TARGETS for extension in EXTENSIONS}
        self.assertEqual([(asset["target"], asset["name"]) for asset in manifest["assets"]],
                         [(target, f"Herdr-{VERSION}-{target}.{extension}")
                          for target in TARGETS for extension in EXTENSIONS])
        self.assertEqual(len(manifest["assets"]), len(expected))
        for asset in manifest["assets"]:
            data = (directory / asset["name"]).read_bytes()
            self.assertEqual(asset["size"], len(data))
            self.assertEqual(asset["sha256"], hashlib.sha256(data).hexdigest())

    def test_existing_legacy_output_is_refused_before_building(self):
        target = TARGETS[0]
        archive = self.fixture.archive(target)
        alias = archive.parent / f"Herdr-{VERSION}-{target}.rpm"
        alias.write_bytes(b"existing package")
        self.fixture.package(target, archive, success=False)
        self.assertEqual(alias.read_bytes(), b"existing package")
        self.assertFalse(self.fixture.log.exists())

    def test_publication_and_verification_require_alias_equality(self):
        version = "20261103.2"
        directory = self.fixture.work / "release"
        directory.mkdir()
        for name in MANIFEST.base_names(version):
            data = b"s" * 64 if name in MANIFEST.UPDATER_SIGNATURES else name.encode()
            (directory / name).write_bytes(data)
        for alias, original in MANIFEST.package_aliases(version).items():
            (directory / alias).write_bytes((directory / original).read_bytes())

        def run(mode, success=True):
            result = subprocess.run(["python3", str(SCRIPTS / "artifact-manifest.py"), mode,
                                     version, str(directory)], capture_output=True, text=True)
            self.assertEqual(result.returncode == 0, success, result.stderr)
            return result

        run("base")
        for name in MANIFEST.base_names(version):
            data = (directory / name).read_bytes()
            for algorithm in ("sha256", "sha512"):
                digest = hashlib.new(algorithm, data).hexdigest()
                (directory / f"{name}.{algorithm}").write_text(f"{digest}  {name}\n")
            for suffix in ("sig", "crt"):
                (directory / f"{name}.{suffix}").write_text("signature fixture\n")
        run("create")
        run("verify")
        self.assertLess(len(list(directory.iterdir())), 128)
        for alias, original in MANIFEST.package_aliases(version).items():
            for algorithm in ("sha256", "sha512"):
                digest = hashlib.new(algorithm, (directory / original).read_bytes()).hexdigest()
                self.assertIn(f"{digest}  {alias}\n", (directory / f"{algorithm.upper()}SUMS").read_text())
            # Alias equality is checked independently of the checksum manifests.
            path = directory / alias
            original_bytes = path.read_bytes()
            path.write_bytes(b"altered package")
            self.assertIn("Package alias differs", run("verify", success=False).stderr)
            path.write_bytes(original_bytes)
            path.unlink()
            run("verify", success=False)
            path.symlink_to(original)
            run("verify", success=False)
            path.unlink()
            path.write_bytes(original_bytes)

    def test_aliases_are_uploaded_and_legacy_inventory_stays_exact(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        upload = workflow.split("name: linux-package-${{ matrix.target }}", 1)[1].split(
            "if-no-files-found:", 1)[0]
        for extension in EXTENSIONS:
            self.assertIn("dist/Herdr-${{ needs.validate.outputs.version }}-${{ matrix.target }}."
                          + extension, upload)
        self.assertFalse(MANIFEST.package_aliases(VERSION, legacy=True))
        legacy = MANIFEST.asset_names(VERSION, legacy=True)
        self.assertFalse(any(name.startswith(("Sheprd-", "sheprd-")) for name in legacy))
