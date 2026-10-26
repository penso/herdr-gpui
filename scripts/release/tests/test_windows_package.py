"""Run: python3 -m unittest discover -s scripts/release/tests -v"""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

SCRIPT = Path(__file__).resolve().parents[1] / "package-windows.py"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PACKAGE = load("package_windows", SCRIPT)
INSTALLER_SCRIPT = SCRIPT.parent / "windows-installer.py"
INSTALLER = load("windows_installer", INSTALLER_SCRIPT)
VERSION = "20260920.3"
TARGET = "x86_64-pc-windows-msvc"
# Stands in for ISCC.exe: checks it was given the zip's tree and writes the
# installer where the defines say.
MOCK_ISCC = """#!/usr/bin/env python3
import pathlib, sys
defines = dict(arg[2:].split("=", 1) for arg in sys.argv[1:] if arg.startswith("/D"))
source = pathlib.Path(defines["SourceDir"])
assert all((source / name).is_file() for name in ("herdr-gpui.exe", "herdr-gpui-launcher.exe", "herdr-acp.exe",
                                              "herdr-remote-control.exe", "herdr-automate.exe"))
assert sys.argv[-1].endswith("herdr-gpui.iss")
name = f"Herdr-{defines['Version']}-{defines['Target']}-setup.exe"
(pathlib.Path(defines["OutputDir"]) / name).write_bytes(b"setup " + defines["Architecture"].encode())
"""


class InstallerTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="herdr-installer-test-")
        self.addCleanup(temp.cleanup)
        self.work = Path(temp.name)
        self.iscc = self.work / "iscc"
        self.iscc.write_text(MOCK_ISCC)
        self.iscc.chmod(0o755)

    def release_zip(self, entries):
        path = self.work / f"Herdr-{VERSION}-{TARGET}.zip"
        with zipfile.ZipFile(path, "w") as archive:
            for name in entries:
                archive.writestr(name, b"data")
        return path

    def build(self, archive, success=True):
        result = subprocess.run([sys.executable, str(INSTALLER_SCRIPT), "build", VERSION, TARGET,
                                 str(archive), str(self.work), str(self.iscc)],
                                capture_output=True, text=True, timeout=60,
                                env={"PATH": os.environ["PATH"]})
        self.assertEqual(result.returncode == 0, success, result.stderr)
        return result

    def test_builds_from_the_zip_tree(self):
        root = f"Herdr-{VERSION}-{TARGET}/"
        archive = self.release_zip([root + "herdr-gpui.exe", root + "herdr-gpui-launcher.exe",
                                    root + "herdr-acp.exe", root + "herdr-remote-control.exe",
                                    root + "herdr-automate.exe", root + "licenses/LICENSE"])
        output = Path(self.build(archive).stdout.strip())
        self.assertEqual(output, self.work.resolve() / f"Herdr-{VERSION}-{TARGET}-setup.exe")
        self.assertEqual(output.read_bytes(), b"setup x64compatible")
        result = self.build(archive, success=False)
        self.assertIn("Output already exists", result.stderr)

    def test_rejects_a_zip_without_the_layout(self):
        root = f"Herdr-{VERSION}-{TARGET}/"
        executables = [root + name for name in ("herdr-gpui.exe", "herdr-gpui-launcher.exe", "herdr-acp.exe",
                                                "herdr-remote-control.exe", "herdr-automate.exe")]
        cases = {
            "no launcher": [e for e in executables if not e.endswith("launcher.exe")],
            "no herdr-acp": [e for e in executables if not e.endswith("herdr-acp.exe")],
            "no herdr-remote-control": [e for e in executables if not e.endswith("herdr-remote-control.exe")],
            "no herdr-automate": [e for e in executables if not e.endswith("herdr-automate.exe")],
            "outside the root": [*executables, "other/x"],
            "escaping the root": [*executables, root + "../x"],
        }
        for name, entries in cases.items():
            with self.subTest(name):
                self.build(self.release_zip(entries), success=False)
        self.assertEqual(list(self.work.glob("*-setup.exe")), [])

    def test_compiler_arguments(self):
        arguments = INSTALLER.iscc_arguments(VERSION, "aarch64-pc-windows-msvc", "src", "out")
        self.assertIn("/DArchitecture=arm64", arguments)
        self.assertIn("/DSourceDir=src", arguments)
        self.assertEqual(Path(arguments[-1]), INSTALLER.SCRIPT)
        self.assertTrue(INSTALLER.SCRIPT.is_file())
        for version, target in [("20260920.03", TARGET), ("v20260920.3", TARGET),
                                (VERSION, "x86_64-pc-windows-gnu")]:
            with self.subTest(version=version, target=target), self.assertRaises(ValueError):
                INSTALLER.iscc_arguments(version, target, "src", "out")

    def test_uninstall_entry_check(self):
        self.assertEqual(INSTALLER.app_id(), "{7FA70894-4FC0-4923-BDCB-B05F7475610A}")
        app = Path("C:/Users/runner/Herdr UI")
        listed = {"DisplayName": "Herdr UI", "UninstallString": f'"{app / "unins000.exe"}"'}
        with mock.patch.object(INSTALLER, "uninstall_entry", return_value=listed):
            INSTALLER.check_uninstall_entry(app)
        hidden = [None, {**listed, "DisplayName": "Herdr"}, {**listed, "SystemComponent": 1},
                  {**listed, "ParentKeyName": "Other"}, {**listed, "UninstallString": '"C:/x/unins000.exe"'}]
        for entry in hidden:
            with self.subTest(entry=entry), mock.patch.object(INSTALLER, "uninstall_entry", return_value=entry), \
                    self.assertRaises(ValueError):
                INSTALLER.check_uninstall_entry(app)

    def test_the_inno_setup_pin_is_one_exact_release(self):
        self.assertRegex(INSTALLER.INNO_SHA256, r"^[0-9a-f]{64}$")
        self.assertIn(f"/is-{INSTALLER.INNO_VERSION.replace('.', '_')}/", INSTALLER.INNO_URL)
        self.assertTrue(INSTALLER.INNO_URL.startswith("https://github.com/jrsoftware/issrc/releases/download/"))


if __name__ == "__main__":
    unittest.main()
