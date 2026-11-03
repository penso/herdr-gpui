"""Renamed publication must remain installable by the deployed bridge."""
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]
SCRIPTS = ROOT / "scripts/release"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


MANIFEST = load("artifact_manifest", SCRIPTS / "artifact-manifest.py")
UPDATER = load("update_manifest", ROOT / "scripts/update-manifest.py")
INSTALLER = load("windows_installer", SCRIPTS / "windows-installer.py")
PACKAGE = load("package_windows", SCRIPTS / "package-windows.py")


class RebrandCompatibility(unittest.TestCase):
    def test_signed_generators_and_publication_describe_the_same_assets(self):
        version = "20261103.1"
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            names = MANIFEST.base_names(version) + list(MANIFEST.package_aliases(version))
            for name in names:
                if not name.endswith(("-manifest.json", "-manifest.sig")):
                    (directory / name).write_bytes(b"artifact")
            for family in ({"require_all_targets": True}, {"packages": True},
                           {"windows": True}, {"hosts": True}):
                payload = json.loads(UPDATER.create(directory, version, **family))
                for asset in payload["assets"]:
                    self.assertIn(asset["name"], names)
                    self.assertEqual(asset["size"], len(b"artifact"))
                    self.assertRegex(asset["name"], r"^Herdr-" if family.get("packages")
                                     else r"^(?:sheprd-|Sheprd-)")

    def test_historical_asset_set_requires_explicit_legacy_mode(self):
        version = "20261102.1"
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            names = MANIFEST.asset_names(version, legacy=True)
            self.assertIn(f"Herdr-{version}-universal-apple-darwin.dmg", names)
            self.assertIn(f"herdr-gpui-{version}-macos-universal.app.tar.gz", names)
            self.assertIn(f"herdr-remote-control-{version}-x86_64-unknown-linux-musl", names)
            (directory / "SHA256SUMS").write_text("".join(f"{'a' * 64}  {name}\n" for name in names))
            self.assertEqual(set(MANIFEST.verify_manifest(directory, version, legacy=True)), set(names))
            with self.assertRaisesRegex(ValueError, "Unexpected checksum manifest"):
                MANIFEST.verify_manifest(directory, version)

    def test_homebrew_renders_the_preserved_destination_and_canonical_command(self):
        rendered = subprocess.run(["bash", str(SCRIPTS / "render-cask.sh"), "20261103.1", "a" * 64],
                                  capture_output=True, text=True, check=True).stdout
        for line in ('cask "herdr-gpui" do', 'name "Sheprd"',
                     'app "Sheprd.app", target: "Herdr.app"',
                     'binary "#{appdir}/Herdr.app/Contents/MacOS/sheprd"'):
            self.assertIn(line, rendered)
        self.assertIn("https://github.com/sheprd-labs/sheprd/releases/", rendered)

    def test_installer_keeps_upgrade_and_startup_identities(self):
        text = INSTALLER.SCRIPT.read_text()
        for line in ('#define AppName "Sheprd"', '#define StartupName "Herdr UI"',
                     '#define AppUserModelID "so.pen.herdr-gpui"', 'AppPublisher=Herdr UI',
                     r'DefaultDirName={localappdata}\Programs\Herdr UI'):
            self.assertIn(line, text)
        self.assertEqual(INSTALLER.app_id(), "{7FA70894-4FC0-4923-BDCB-B05F7475610A}")
        startup = [line for line in text.splitlines() if line.startswith("Root:") and
                   ("CurrentVersion\\Run" in line or "StartupApproved\\Run" in line)]
        self.assertTrue(startup)
        for line in startup:
            self.assertIn('ValueName: "{#StartupName}"', line)
            self.assertIn("uninsdeletevalue dontcreatekey", line)
            self.assertNotIn("ValueData:", line)
        self.assertIn("CompareText(RemoveBackslashUnlessRoot(ExtractFilePath(Target))", text)
        self.assertIn("FileExists(NewPath) or FileCopy(OldPath, NewPath, True)", text)

    def test_every_packaged_executable_and_alias_is_signed_and_installed(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        signing = workflow.split("      - uses: azure/artifact-signing-action@", 1)[1].split("      - name:", 1)[0]
        names = set(re.findall(r"\\([^\\\s]+\.exe)$", signing, re.M))
        self.assertEqual(names, set(PACKAGE.EXECUTABLES))
        text = INSTALLER.SCRIPT.read_text()
        for name in PACKAGE.EXECUTABLES:
            definition = {"sheprd.exe": "{#AppExe}", "sheprd-launcher.exe": "{#Launcher}",
                          "sheprd-automate.exe": "{#Automate}"}.get(name, name)
            self.assertIn(f'Source: "{{#SourceDir}}\\{definition}";', text)

    def test_native_smoke_preserves_a_disabled_legacy_startup_entry(self):
        launcher = Path("C:/Programs/Herdr UI/herdr-gpui-launcher.exe")
        values = {INSTALLER.STARTUP_KEYS[0]: {"Herdr UI": (f'"{launcher}"', 1)},
                  INSTALLER.STARTUP_KEYS[1]: {"Herdr UI": (bytes([3]) + bytes(11), 3)}}

        def query(key, name):
            if name not in values[key]:
                raise FileNotFoundError(name)
            return values[key][name]

        def open_key(_, name):
            context = mock.MagicMock()
            context.__enter__.return_value = name
            return context

        registry = types.SimpleNamespace(HKEY_CURRENT_USER=0, REG_SZ=1, REG_BINARY=3,
                                         OpenKey=open_key, QueryValueEx=query)
        with mock.patch.dict(sys.modules, {"winreg": registry}):
            INSTALLER.check_startup_preserved(launcher)
            values[INSTALLER.STARTUP_KEYS[1]]["Herdr UI"] = (bytes([2]) + bytes(11), 3)
            with self.assertRaisesRegex(ValueError, "disabled state"):
                INSTALLER.check_startup_preserved(launcher)
            values[INSTALLER.STARTUP_KEYS[1]]["Herdr UI"] = (bytes([3]) + bytes(11), 3)
            values[INSTALLER.STARTUP_KEYS[0]]["Sheprd"] = (f'"{launcher}"', 1)
            with self.assertRaisesRegex(ValueError, "second startup identity"):
                INSTALLER.check_startup_preserved(launcher)
