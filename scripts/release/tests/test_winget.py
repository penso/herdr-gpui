"""Run: python3 -m unittest discover -s scripts/release/tests -v"""
import importlib.util
from pathlib import Path
import re
import tempfile
import unittest
from unittest import mock


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SCRIPTS = Path(__file__).resolve().parents[1]
WINGET = load("winget", SCRIPTS / "winget.py")
VERSION = "20260920.3"
X64 = "a" * 64
ARM64 = "b" * 64
RELEASE = f"https://github.com/sheprd-labs/sheprd/releases/download/v{VERSION}"
VERSION_MANIFEST = f"""# Created with komac v2.16.0
PackageIdentifier: Penso.HerdrUI
PackageVersion: '{VERSION}'
DefaultLocale: en-US
ManifestType: version
ManifestVersion: 1.10.0
"""
LOCALE = f"""PackageIdentifier: Penso.HerdrUI
PackageVersion: '{VERSION}'
PackageLocale: en-US
Publisher: Herdr UI
PackageName: Sheprd
Tags:
- terminal
ManifestType: defaultLocale
ManifestVersion: 1.10.0
"""
# Komac hoists what all installers share to the top level.
INSTALLER = f"""# yaml-language-server: $schema=https://aka.ms/winget-manifest.installer.1.10.0.schema.json

PackageIdentifier: Penso.HerdrUI
PackageVersion: '{VERSION}'
InstallerType: inno
Scope: user
UpgradeBehavior: install
ProductCode: '{{7FA70894-4FC0-4923-BDCB-B05F7475610A}}_is1'
Installers:
- Architecture: x64
  InstallerUrl: {RELEASE}/Sheprd-{VERSION}-x86_64-pc-windows-msvc-setup.exe
  InstallerSha256: {X64.upper()}
  AppsAndFeaturesEntries:
  - DisplayName: Sheprd
    Publisher: Herdr UI
- Architecture: arm64
  InstallerUrl: {RELEASE}/Sheprd-{VERSION}-aarch64-pc-windows-msvc-setup.exe
  InstallerSha256: {ARM64.upper()}
ManifestType: installer
ManifestVersion: 1.10.0
"""


class WingetTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="herdr-winget-test-")
        self.addCleanup(temp.cleanup)
        self.work = Path(temp.name)
        self.sums = self.work / "SHA256SUMS"
        self.write_sums(arm64=True)
        self.output = self.work / "out"
        self.directory = self.output / "manifests/p/Penso/HerdrUI" / VERSION
        self.directory.mkdir(parents=True)
        self.write(VERSION_MANIFEST, LOCALE, INSTALLER)

    def write_sums(self, arm64):
        lines = [f"{'c' * 64}  Sheprd-{VERSION}-x86_64-pc-windows-msvc.zip",
                 f"{X64}  Sheprd-{VERSION}-x86_64-pc-windows-msvc-setup.exe"]
        if arm64:
            lines.append(f"{ARM64}  Sheprd-{VERSION}-aarch64-pc-windows-msvc-setup.exe")
        self.sums.write_text("\n".join(lines) + "\n")

    def write(self, version, locale, installer):
        (self.directory / "Penso.HerdrUI.yaml").write_text(version)
        (self.directory / "Penso.HerdrUI.locale.en-US.yaml").write_text(locale)
        (self.directory / "Penso.HerdrUI.installer.yaml").write_text(installer)

    def test_installers_come_from_the_published_checksums(self):
        self.assertEqual(WINGET.installers(VERSION, self.sums), {
            "x64": (f"{RELEASE}/Sheprd-{VERSION}-x86_64-pc-windows-msvc-setup.exe", X64),
            "arm64": (f"{RELEASE}/Sheprd-{VERSION}-aarch64-pc-windows-msvc-setup.exe", ARM64)})
        self.write_sums(arm64=False)
        self.assertEqual(list(WINGET.installers(VERSION, self.sums)), ["x64"])
        self.sums.write_text(f"{ARM64}  Sheprd-{VERSION}-aarch64-pc-windows-msvc-setup.exe\n")
        with self.assertRaisesRegex(ValueError, "x86_64"):
            WINGET.installers(VERSION, self.sums)
        with self.assertRaises(ValueError):
            WINGET.installers("v" + VERSION, self.sums)

    def test_accepts_komacs_manifests_for_exactly_the_release(self):
        self.assertEqual(WINGET.check(VERSION, self.sums, self.output), self.directory)
        # Per-installer fields count as much as hoisted ones.
        moved = INSTALLER.replace("InstallerType: inno\nScope: user\n", "").replace(
            "- Architecture: x64\n", "- Architecture: x64\n  InstallerType: inno\n  Scope: user\n").replace(
            "- Architecture: arm64\n", "- Architecture: arm64\n  Scope: user\n  InstallerType: inno\n")
        self.write(VERSION_MANIFEST, LOCALE, moved)
        WINGET.check(VERSION, self.sums, self.output)

    def test_rejects_manifests_that_differ_from_the_release(self):
        x64_url = f"{RELEASE}/Sheprd-{VERSION}-x86_64-pc-windows-msvc-setup.exe"
        cases = {
            "machine scope": ("installer", "Scope: user", "Scope: machine"),
            "not inno": ("installer", "InstallerType: inno", "InstallerType: exe"),
            "wrong digest": ("installer", X64.upper(), "D" * 64),
            "wrong url": ("installer", x64_url, x64_url.replace("v2026", "v2025")),
            "missing arm64": ("installer", INSTALLER[INSTALLER.index("- Architecture: arm64"):
                                                    INSTALLER.index("ManifestType")], ""),
            "duplicate x64": ("installer", "Architecture: arm64", "Architecture: x64"),
            "new product code": ("installer", "_is1", "_is2"),
            "renamed identifier": ("installer", "PackageIdentifier: Penso.HerdrUI", "PackageIdentifier: Herdr.UI"),
            "other version": ("version", f"'{VERSION}'", "'20260920.2'"),
            "new publisher": ("locale", "Publisher: Herdr UI", "Publisher: Someone"),
            "no publisher": ("locale", "Publisher: Herdr UI\n", ""),
            "old display name": ("locale", "PackageName: Sheprd", "PackageName: Herdr UI"),
        }
        for name, (manifest, old, new) in cases.items():
            texts = {"version": VERSION_MANIFEST, "locale": LOCALE, "installer": INSTALLER}
            self.assertIn(old, texts[manifest], name)
            texts[manifest] = texts[manifest].replace(old, new)
            self.write(texts["version"], texts["locale"], texts["installer"])
            with self.subTest(name), self.assertRaises(ValueError):
                WINGET.check(VERSION, self.sums, self.output)
        self.write(VERSION_MANIFEST, LOCALE, INSTALLER)
        (self.directory / "Penso.HerdrUI.locale.en-US.yaml").unlink()
        with self.assertRaisesRegex(ValueError, "default locale"):
            WINGET.check(VERSION, self.sums, self.output)

    def test_identity_matches_the_installer(self):
        script = (SCRIPTS / "sheprd.iss").read_text()
        self.assertIn(f"\nAppPublisher={WINGET.PUBLISHER}\n", script)
        app_id = re.search(r"^AppId=\{(\{[0-9A-F-]+\})$", script, re.M)[1]
        self.assertEqual(WINGET.PRODUCT_CODE, app_id + "_is1")
        self.assertIn("\nPrivilegesRequired=lowest\n", script)

    def test_submit_refreshes_branding_without_changing_upgrade_identity(self):
        output = self.work / "generated"

        def komac(arguments, **_):
            if arguments[1] == "update":
                self.output = output
                self.directory = output / "manifests/p/Penso/HerdrUI" / VERSION
                self.directory.mkdir(parents=True)
                self.write(VERSION_MANIFEST, LOCALE.replace("PackageName: Sheprd", "PackageName: Herdr UI")
                           + "PackageUrl: https://herdrui.dev\n", INSTALLER)
            else:
                self.assertEqual(arguments, ["komac", "submit", "--yes", str(self.directory)])

        with mock.patch.dict(WINGET.os.environ, {"GITHUB_TOKEN": "test"}), \
                mock.patch.object(WINGET, "published", return_value=True), \
                mock.patch.object(WINGET.subprocess, "run", side_effect=komac) as run:
            WINGET.submit(VERSION, self.sums, Path("komac"), output)
        self.assertEqual([call.args[0][1] for call in run.call_args_list], ["update", "submit"])
        locale = (self.directory / "Penso.HerdrUI.locale.en-US.yaml").read_text()
        self.assertIn("Publisher: Herdr UI\n", locale)
        self.assertIn("PackageName: Sheprd\n", locale)
        self.assertIn("PackageUrl: https://sheprd.org\n", locale)

    def test_submit_without_a_token_warns_and_runs_nothing(self):
        env = {key: value for key, value in WINGET.os.environ.items() if key != "GITHUB_TOKEN"}
        with mock.patch.dict(WINGET.os.environ, env, clear=True), \
                mock.patch.object(WINGET, "published") as published, \
                mock.patch.object(WINGET.subprocess, "run") as run, \
                mock.patch("builtins.print") as printed:
            WINGET.submit(VERSION, self.sums, Path("komac"), self.work / "unused")
        published.assert_not_called()
        run.assert_not_called()
        self.assertIn("::warning::WINGET_TOKEN is not set", printed.call_args.args[0])

    def test_the_komac_pin_is_one_exact_release(self):
        self.assertRegex(WINGET.KOMAC_SHA256, r"^[0-9a-f]{64}$")
        self.assertTrue(WINGET.KOMAC_URL.startswith(
            f"https://github.com/russellbanks/Komac/releases/download/v{WINGET.KOMAC_VERSION}/"))


if __name__ == "__main__":
    unittest.main()
