"""Run: python3 -m unittest discover -s scripts/release/tests -v"""
import json
import os
from pathlib import Path
import subprocess
import shutil
import tarfile
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[3]
SCRIPTS = ROOT / "scripts/release"


def build_identity(worktree=False):
    return (b"not executable\0HERDR_BUILD_IDENTITY_V1\nworktree="
            + (b"1\nbranch=feature/test\npr=42\n\0" if worktree else b"0\nbranch=\npr=\n\0"))


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="herdr-release-test-")
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        self.env = {"PATH": os.environ["PATH"], "HOME": str(self.work)}
        self.notices = self.work / "third party notices.txt"
        self.notices.write_text("Third-party license text\nCopyright example\n")

    def run_script(self, name, *args, success=True):
        result = subprocess.run(
            ["bash", str(getattr(self, "scripts", SCRIPTS) / name), *map(str, args)],
            env=self.env, capture_output=True, text=True, timeout=30,
        )
        if success:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0)
        return result

    def test_cask(self):
        first = self.run_script("render-cask.sh", "20260920.3", "AB" * 32).stdout
        self.assertEqual(first, self.run_script("render-cask.sh", "20260920.3", "ab" * 32).stdout)
        self.assertIn('version "20260920.3"', first)
        self.assertIn('sha256 "' + "ab" * 32 + '"', first)
        self.assertIn("penso/herdr-gpui/releases/download/v#{version}/Herdr-#{version}-universal-apple-darwin.dmg", first)
        # Symbol form, not the deprecated string comparison. Cask `depends_on
        # macos:` always parses with comparator ">=", so this still means
        # "Sonoma or newer" and does not pin an exact release.
        self.assertIn("depends_on macos: :sonoma", first)
        for version in ["v20260920.3", "1.2", "020260920.3", "20260920.3-rc1", "20260920.3+build", "20260920.3\n", "$(id)"]:
            self.run_script("render-cask.sh", version, "ab" * 32, success=False)
        for sha in ["abc", "g" * 64, "a" * 65, "a" * 63 + "\n"]:
            self.run_script("render-cask.sh", "20260920.3", sha, success=False)

    def test_linux_archive(self):
        binary = self.work / "input binary"
        binary.write_bytes(build_identity())
        acp = self.work / "herdr-acp"
        acp.write_bytes(b"chat program")
        remote_control = self.work / "herdr-remote-control"
        remote_control.write_bytes(b"remote control daemon")
        automate = self.work / "herdr-automate"
        automate.write_bytes(b"automation daemon")
        core = self.work / "herdr-core"
        core.write_bytes(b"herdr-core daemon")
        for target in ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"):
            with self.subTest(target=target):
                result = self.run_script("package-linux.sh", "20260920.3", target, binary, self.work, self.notices)
                with tarfile.open(result.stdout.strip()) as archive:
                    base = f"Herdr-20260920.3-{target}/"
                    files = {m.name: m for m in archive.getmembers() if m.isfile()}
                    self.assertEqual(set(files), {base + p for p in [
                        "bin/herdr-gpui", "bin/herdr-acp", "bin/herdr-remote-control", "bin/herdr-automate",
                        "bin/herdr-core",
                        "share/applications/so.pen.herdr-gpui.desktop",
                        "share/icons/hicolor/scalable/apps/herdr-gpui.svg",
                        "share/licenses/herdr-gpui/LICENSE-APACHE", "share/licenses/herdr-gpui/NOTICE.md",
                        "share/licenses/herdr-gpui/LICENSE", "share/licenses/herdr-gpui/NOTICE",
                        "share/licenses/herdr-gpui/LICENSE-octicons",
                        "share/licenses/herdr-gpui/THIRD-PARTY-NOTICES.txt",
                        "share/licenses/herdr-gpui/SOUND-NOTICE.md",
                        "share/licenses/herdr-gpui/HERDR-CORE-LICENSES.txt",
                    ]})
                    self.assertEqual(files[base + "bin/herdr-gpui"].mode & 0o777, 0o755)
                    self.assertEqual(archive.extractfile(base + "bin/herdr-gpui").read(), binary.read_bytes())
                    self.assertEqual(files[base + "bin/herdr-acp"].mode & 0o777, 0o755)
                    self.assertEqual(archive.extractfile(base + "bin/herdr-acp").read(), acp.read_bytes())
                    self.assertEqual(files[base + "bin/herdr-remote-control"].mode & 0o777, 0o755)
                    self.assertEqual(archive.extractfile(base + "bin/herdr-remote-control").read(),
                                     remote_control.read_bytes())
                    self.assertEqual(files[base + "bin/herdr-automate"].mode & 0o777, 0o755)
                    self.assertEqual(archive.extractfile(base + "bin/herdr-automate").read(), automate.read_bytes())
                    self.assertEqual(files[base + "bin/herdr-core"].mode & 0o777, 0o755)
                    self.assertEqual(archive.extractfile(base + "bin/herdr-core").read(), core.read_bytes())
                    self.assertEqual(archive.extractfile(base + "share/licenses/herdr-gpui/HERDR-CORE-LICENSES.txt").read(),
                                     (SCRIPTS / "herdr-core-licenses.txt").read_bytes())
                    self.assertEqual(archive.extractfile(base + "share/icons/hicolor/scalable/apps/herdr-gpui.svg").read(), (ROOT / "assets/icons/herdr-linux.svg").read_bytes())
                    self.assertEqual(archive.extractfile(base + "share/licenses/herdr-gpui/THIRD-PARTY-NOTICES.txt").read(), self.notices.read_bytes())
                    for source in ("LICENSE", "NOTICE", "assets/icons/LICENSE-octicons", "crates/herdr-protocol/NOTICE.md", "crates/herdr-gpui/SOUND-NOTICE.md"):
                        self.assertEqual(archive.extractfile(base + "share/licenses/herdr-gpui/" + Path(source).name).read(), (ROOT / source).read_bytes())
                # The updater ships the same binary, without changing the manual tree.
                update = self.work / f"herdr-gpui-20260920.3-{target}-update.tar.gz"
                subprocess.run(["python3", str(ROOT / "scripts/update-manifest.py"), "package-linux",
                                str(binary), target, "20260920.3", str(update)], check=True,
                               env=self.env, capture_output=True)
                with tarfile.open(update) as archive:
                    self.assertEqual(archive.getnames(), [f"herdr-gpui-20260920.3-{target}"])
                    self.assertEqual(archive.extractfile(archive.getmembers()[0]).read(), binary.read_bytes())
                self.run_script("package-linux.sh", "20260920.3", target, binary, self.work, self.notices, success=False)
        self.run_script("package-linux.sh", "20260920.3", "bad-target", binary, self.work, self.notices, success=False)
        acp.unlink()
        result = self.run_script("package-linux.sh", "20260920.3", "x86_64-unknown-linux-gnu", binary, self.work,
                                 self.notices, success=False)
        self.assertIn("A herdr-acp beside the binary required", result.stderr)
        acp.write_bytes(b"chat program")
        remote_control.unlink()
        result = self.run_script("package-linux.sh", "20260920.3", "x86_64-unknown-linux-gnu", binary, self.work,
                                 self.notices, success=False)
        self.assertIn("A herdr-remote-control beside the binary required", result.stderr)
        remote_control.write_bytes(b"remote control daemon")
        automate.unlink()
        result = self.run_script("package-linux.sh", "20260920.3", "x86_64-unknown-linux-gnu", binary, self.work,
                                 self.notices, success=False)
        self.assertIn("A herdr-automate beside the binary required", result.stderr)
        automate.write_bytes(b"automation daemon")
        core.unlink()
        result = self.run_script("package-linux.sh", "20260920.3", "x86_64-unknown-linux-gnu", binary, self.work,
                                 self.notices, success=False)
        self.assertIn("A herdr-core beside the binary required", result.stderr)

    def run_windows(self, *args, success=True):
        result = subprocess.run(["python3", str(SCRIPTS / "package-windows.py"), *map(str, args)],
                                env=self.env, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode == 0, success, result.stderr)
        return result

    def test_windows_archive(self):
        binary = self.work / "input binary.exe"
        binary.write_bytes(build_identity())
        launcher = self.work / "input launcher.exe"
        launcher.write_bytes(b"windowed launcher")
        acp = self.work / "herdr-acp.exe"
        acp.write_bytes(b"chat program")
        remote_control = self.work / "herdr-remote-control.exe"
        remote_control.write_bytes(b"remote control daemon")
        automate = self.work / "herdr-automate.exe"
        automate.write_bytes(b"automation daemon")
        core = self.work / "herdr-core.exe"
        core.write_bytes(b"herdr-core daemon")
        target = "x86_64-pc-windows-msvc"
        output = Path(self.run_windows("20260920.3", target, binary, launcher, self.work, self.notices).stdout.strip())
        self.assertEqual(output, self.work.resolve() / f"Herdr-20260920.3-{target}.zip")
        base = f"Herdr-20260920.3-{target}/"
        with zipfile.ZipFile(output) as archive:
            self.assertIsNone(archive.testzip())
            self.assertEqual(set(archive.namelist()), {base + p for p in [
                "herdr-gpui.exe", "herdr-gpui-launcher.exe", "herdr-acp.exe", "herdr-remote-control.exe", "herdr-automate.exe", "herdr-core.exe", "licenses/LICENSE", "licenses/NOTICE", "licenses/LICENSE-octicons",
                "licenses/LICENSE-APACHE", "licenses/NOTICE.md", "licenses/SOUND-NOTICE.md",
                "licenses/THIRD-PARTY-NOTICES.txt", "licenses/HERDR-CORE-LICENSES.txt",
            ]})
            self.assertEqual({info.date_time for info in archive.infolist()}, {(1980, 1, 1, 0, 0, 0)})
            self.assertEqual(archive.read(base + "herdr-gpui.exe"), binary.read_bytes())
            self.assertEqual(archive.read(base + "herdr-gpui-launcher.exe"), launcher.read_bytes())
            self.assertEqual(archive.read(base + "herdr-acp.exe"), acp.read_bytes())
            self.assertEqual(archive.read(base + "herdr-remote-control.exe"), remote_control.read_bytes())
            self.assertEqual(archive.read(base + "herdr-automate.exe"), automate.read_bytes())
            self.assertEqual(archive.read(base + "herdr-core.exe"), core.read_bytes())
            self.assertEqual(archive.read(base + "licenses/HERDR-CORE-LICENSES.txt"),
                             (SCRIPTS / "herdr-core-licenses.txt").read_bytes())
            self.assertEqual(archive.read(base + "licenses/THIRD-PARTY-NOTICES.txt"), self.notices.read_bytes())
            for source in ("LICENSE", "NOTICE", "assets/icons/LICENSE-octicons", "crates/herdr-protocol/LICENSE-APACHE",
                           "crates/herdr-protocol/NOTICE.md", "crates/herdr-gpui/SOUND-NOTICE.md"):
                self.assertEqual(archive.read(base + "licenses/" + Path(source).name), (ROOT / source).read_bytes())
        before = output.read_bytes()
        result = self.run_windows("20260920.3", target, binary, launcher, self.work, self.notices, success=False)
        self.assertIn("Output already exists", result.stderr)
        self.assertEqual(output.read_bytes(), before)
        output.unlink()
        arm = Path(self.run_windows("20260920.3", "aarch64-pc-windows-msvc", binary, launcher, self.work,
                                    self.notices).stdout.strip())
        self.assertEqual(arm, self.work.resolve() / "Herdr-20260920.3-aarch64-pc-windows-msvc.zip")
        arm.unlink()
        result = self.run_windows("20260920.3", target, binary, self.work / "missing-launcher.exe", self.work,
                                  self.notices, success=False)
        self.assertIn("Launcher file required", result.stderr)
        acp.unlink()
        result = self.run_windows("20260920.3", target, binary, launcher, self.work, self.notices, success=False)
        self.assertIn("A herdr-acp.exe beside the binary required", result.stderr)
        acp.write_bytes(b"chat program")
        remote_control.unlink()
        result = self.run_windows("20260920.3", target, binary, launcher, self.work, self.notices, success=False)
        self.assertIn("A herdr-remote-control.exe beside the binary required", result.stderr)
        remote_control.write_bytes(b"remote control daemon")
        automate.unlink()
        result = self.run_windows("20260920.3", target, binary, launcher, self.work, self.notices, success=False)
        self.assertIn("A herdr-automate.exe beside the binary required", result.stderr)
        automate.write_bytes(b"automation daemon")
        core.unlink()
        result = self.run_windows("20260920.3", target, binary, launcher, self.work, self.notices, success=False)
        self.assertIn("A herdr-core.exe beside the binary required", result.stderr)
        core.write_bytes(b"herdr-core daemon")
        for args in [("20260920.03", target, binary, launcher, self.work, self.notices),
                     ("v20260920.3", target, binary, launcher, self.work, self.notices),
                     ("20260920.3", "x86_64-pc-windows-gnu", binary, launcher, self.work, self.notices),
                     ("20260920.3", target, self.work / "missing.exe", launcher, self.work, self.notices),
                     ("20260920.3", target, binary, self.work, self.work, self.notices),
                     ("20260920.3", target, binary, launcher, self.work / "missing", self.notices),
                     ("20260920.3", target, binary, launcher, self.work, self.work / "missing"),
                     ("20260920.3", target, binary, launcher, self.work, self.work / "empty")]:
            (self.work / "empty").touch()
            with self.subTest(args=args):
                self.run_windows(*args, success=False)
        self.assertEqual([p.name for p in self.work.iterdir() if p.suffix == ".zip" or p.name.startswith(".herdr")], [])

    def test_packaging_requires_notices(self):
        self.mock_tools()
        binary = self.work / "binary"
        binary.touch()
        for script, inputs in [("package-linux.sh", ["x86_64-unknown-linux-gnu", binary]),
                               ("package-macos.sh", [binary, binary])]:
            self.run_script(script, "20260920.3", *inputs, self.work, success=False)
            for notices in [self.work / "missing", binary]:
                result = self.run_script(script, "20260920.3", *inputs, self.work, notices, success=False)
                self.assertIn("Nonempty third-party notices", result.stderr)
        self.assertFalse((self.work / "Herdr.app").exists())
        self.assertFalse(list(self.work.glob("*.tar.gz")))

    def mock_tools(self):
        tools = self.work / "tools"
        tools.mkdir()
        mock = tools / "mock.py"
        mock.write_bytes((SCRIPTS / "tests/mock-tool.py").read_bytes())
        mock.chmod(0o755)
        for tool in ["uname", "security", "openssl", "base64", "plutil", "lipo", "ditto", "xcrun", "hdiutil", "codesign", "spctl"]:
            (tools / tool).symlink_to(mock)
        self.env.update(PATH=str(tools) + os.pathsep + self.env["PATH"], MOCK_LOG=str(self.work / "log"))

    def test_macos_requires_darwin(self):
        self.mock_tools()
        self.env["MOCK_OS"] = "Linux"
        for script, args in [
            ("package-macos.sh", ["20260920.3", "arm", "intel", self.work, self.notices]),
            ("sign-macos.sh", ["20260920.3", "app", self.work]),
        ]:
            result = self.run_script(script, *args, success=False)
            self.assertIn("macOS required", result.stderr)
        self.assertFalse((self.work / "log").exists())

    def macos_builds(self, arm_worktree=False, intel_worktree=False):
        """Each architecture's build folder, as the build leaves it:
        herdr-gpui and the herdr-acp, herdr-remote-control, herdr-automate,
        and fetched herdr-core beside it, in a folder the mock lipo reads the
        architecture from."""
        binaries = []
        for arch, worktree in (("arm64", arm_worktree), ("x86_64", intel_worktree)):
            folder = self.work / arch
            folder.mkdir(exist_ok=True)
            (folder / "herdr-gpui").write_bytes(build_identity(worktree))
            (folder / "herdr-acp").write_bytes(b"chat program")
            (folder / "herdr-remote-control").write_bytes(b"remote control daemon")
            (folder / "herdr-automate").write_bytes(b"automation daemon")
            (folder / "herdr-core").write_bytes(b"herdr-core daemon")
            binaries.append(folder / "herdr-gpui")
        return binaries

    def test_unsigned_assembly(self):
        self.mock_tools()
        arm, intel = self.macos_builds()
        self.run_script("package-macos.sh", "20260920.3", arm, intel, self.work, self.notices)
        app = self.work / "Herdr.app"
        self.assertEqual({str(p.relative_to(app)) for p in app.rglob("*") if p.is_file()}, {
            "Contents/MacOS/Herdr", "Contents/MacOS/herdr-acp", "Contents/MacOS/herdr-remote-control",
            "Contents/MacOS/herdr-automate", "Contents/MacOS/herdr-core",
            "Contents/Info.plist", "Contents/Resources/Herdr.icns",
            "Contents/Resources/Assets.car",
            "Contents/Resources/LICENSE-APACHE", "Contents/Resources/NOTICE.md",
            "Contents/Resources/LICENSE", "Contents/Resources/NOTICE", "Contents/Resources/LICENSE-octicons",
            "Contents/Resources/THIRD-PARTY-NOTICES.txt",
            "Contents/Resources/SOUND-NOTICE.md",
            "Contents/Resources/HERDR-CORE-LICENSES.txt",
        })
        self.assertEqual((app / "Contents/Resources/THIRD-PARTY-NOTICES.txt").read_bytes(), self.notices.read_bytes())
        self.assertEqual((app / "Contents/Resources/HERDR-CORE-LICENSES.txt").read_bytes(),
                         (SCRIPTS / "herdr-core-licenses.txt").read_bytes())
        self.assertEqual((app / "Contents/Resources/Herdr.icns").read_bytes(), (ROOT / "assets/icons/Herdr.icns").read_bytes())
        self.assertEqual((app / "Contents/Resources/Assets.car").read_bytes(), (ROOT / "assets/icons/Herdr.car").read_bytes())
        for source in ("LICENSE", "NOTICE", "assets/icons/LICENSE-octicons", "crates/herdr-protocol/NOTICE.md", "crates/herdr-gpui/SOUND-NOTICE.md"):
            self.assertEqual((app / "Contents/Resources" / Path(source).name).read_bytes(), (ROOT / source).read_bytes())
        self.run_script("package-macos.sh", "20260920.3", arm, intel, self.work, self.notices, success=False)

    def test_assembly_requires_each_companion_beside_each_binary(self):
        self.mock_tools()
        for companion in ("herdr-acp", "herdr-remote-control", "herdr-automate", "herdr-core"):
            with self.subTest(companion=companion):
                arm, intel = self.macos_builds()
                (intel.parent / companion).unlink()
                result = self.run_script("package-macos.sh", "20260920.3", arm, intel, self.work, self.notices,
                                         success=False)
                self.assertIn(f"A {companion} beside each binary required", result.stderr)
                self.assertFalse((self.work / "Herdr.app").exists())

    def test_worktree_icons_and_mismatched_architectures(self):
        self.mock_tools()
        # Distinct fixture artwork proves selection without generating real icons.
        fixture = self.work / "packaging checkout"
        for path in ["scripts/release/common.sh", "scripts/release/package-macos.sh",
                     "scripts/release/package-linux.sh", "scripts/release/build-icon.py",
                     "scripts/release/so.pen.herdr-gpui.desktop", "assets/macos/Info.plist",
                      "LICENSE", "NOTICE", "assets/icons/LICENSE-octicons",
                      "crates/herdr-gpui/SOUND-NOTICE.md", "scripts/release/herdr-core-licenses.txt",
                     "crates/herdr-protocol/LICENSE-APACHE", "crates/herdr-protocol/NOTICE.md"]:
            destination = fixture / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / path, destination)
        for name in ["Herdr-worktree.icns", "Herdr-worktree.car", "herdr-linux-worktree.svg"]:
            (fixture / "assets/icons" / name).write_bytes(b"red fixture " + name.encode())
        self.scripts = fixture / "scripts/release"
        arm, intel = self.macos_builds(arm_worktree=True)
        result = self.run_script("package-macos.sh", "20260920.3", arm, intel, self.work, self.notices, success=False)
        self.assertIn("different build identities", result.stderr)
        intel.write_bytes(build_identity(True))
        self.run_script("package-macos.sh", "20260920.3", arm, intel, self.work, self.notices)
        self.assertEqual((self.work / "Herdr.app/Contents/Resources/Herdr.icns").read_bytes(), (fixture / "assets/icons/Herdr-worktree.icns").read_bytes())
        self.assertEqual((self.work / "Herdr.app/Contents/Resources/Assets.car").read_bytes(), (fixture / "assets/icons/Herdr-worktree.car").read_bytes())
        result = self.run_script("package-linux.sh", "20260920.3", "aarch64-unknown-linux-gnu", arm, self.work, self.notices)
        with tarfile.open(result.stdout.strip()) as archive:
            self.assertEqual(archive.extractfile("Herdr-20260920.3-aarch64-unknown-linux-gnu/share/icons/hicolor/scalable/apps/herdr-gpui.svg").read(), (fixture / "assets/icons/herdr-linux-worktree.svg").read_bytes())

    def test_missing_malformed_conflicting_identity_fails_closed(self):
        binary = self.work / "binary"
        for data in [b"old binary", build_identity() + build_identity(True),
                     build_identity().replace(b"pr=\n", b"pr=0\n"),
                     build_identity().replace(b"worktree=0", b"worktree=9")]:
            binary.write_bytes(data)
            self.run_script("package-linux.sh", "20260920.3", "x86_64-unknown-linux-gnu", binary, self.work, self.notices, success=False)
            self.assertFalse(list(self.work.glob("*.tar.gz")))

    def test_signing_retries_apple_network_failures(self):
        self.mock_tools()
        app = self.work / "unsigned.app"
        (app / "Contents/MacOS").mkdir(parents=True)
        (app / "Contents/MacOS/Herdr").write_text("never run")
        (app / "Contents/MacOS/herdr-acp").write_text("never run")
        (app / "Contents/MacOS/herdr-remote-control").write_text("never run")
        (app / "Contents/MacOS/herdr-automate").write_text("never run")
        (app / "Contents/MacOS/herdr-core").write_text("never run")
        (app / "Contents/Resources").mkdir()
        (app / "Contents/Resources/THIRD-PARTY-NOTICES.txt").write_bytes(self.notices.read_bytes())
        self.env.update(
            MACOS_CERTIFICATE_P12_BASE64="ZHVtbXk=", MACOS_CERTIFICATE_PASSWORD="dummy",
            APPLE_API_PRIVATE_KEY="dummy p8", APPLE_API_KEY_ID="dummy",
            APPLE_API_ISSUER_ID="dummy", MACOS_SIGNING_IDENTITY="Developer ID Application: Dummy",
            APPLE_RETRY_DELAY="0",
        )
        output = self.work / "Herdr-20260920.3-universal-apple-darwin.dmg"
        # Nine dropped connections in a row still succeed on the tenth try.
        self.env["MOCK_XCRUN_FAILURES"] = "9"
        result = self.run_script("sign-macos.sh", "20260920.3", app, self.work)
        self.assertIn("attempt 9 of 10", result.stderr)
        self.assertTrue(output.exists())
        for path in (output, self.work / "herdr-gpui-20260920.3-macos-universal.app.tar.gz",
                     self.work / "xcrun-failures"):
            path.unlink()
        # Ten in a row give up, leaving no output behind.
        self.env["MOCK_XCRUN_FAILURES"] = "10"
        result = self.run_script("sign-macos.sh", "20260920.3", app, self.work, success=False)
        self.assertNotIn("attempt 10 of 10", result.stderr)
        self.assertFalse(output.exists())
        self.assertFalse(list(self.work.glob(".herdr-sign.*")))
        (self.work / "xcrun-failures").unlink()
        self.env.update(MOCK_XCRUN_FAILURES="0", APPLE_RETRY_DELAY="soon")
        self.run_script("sign-macos.sh", "20260920.3", app, self.work, success=False)
        self.assertFalse(output.exists())

    def test_signing_success_and_fail_closed(self):
        self.mock_tools()
        app = self.work / "unsigned.app"
        (app / "Contents/MacOS").mkdir(parents=True)
        (app / "Contents/MacOS/Herdr").write_text("never run")
        (app / "Contents/MacOS/herdr-acp").write_text("never run")
        (app / "Contents/MacOS/herdr-remote-control").write_text("never run")
        (app / "Contents/MacOS/herdr-automate").write_text("never run")
        (app / "Contents/MacOS/herdr-core").write_text("never run")
        (app / "Contents/Resources").mkdir()
        (app / "Contents/Resources/THIRD-PARTY-NOTICES.txt").write_bytes(self.notices.read_bytes())
        self.env.update(
            MACOS_CERTIFICATE_P12_BASE64="ZHVtbXk=", MACOS_CERTIFICATE_PASSWORD="dummy",
            APPLE_API_PRIVATE_KEY="dummy p8", APPLE_API_KEY_ID="dummy",
            APPLE_API_ISSUER_ID="dummy", MACOS_SIGNING_IDENTITY="Developer ID Application: Dummy",
            APPLE_RETRY_DELAY="0",
        )
        output = self.work / "Herdr-20260920.3-universal-apple-darwin.dmg"
        update = self.work / "herdr-gpui-20260920.3-macos-universal.app.tar.gz"
        for response in ['{"status":"Invalid"}', '{"status":"In Progress"}', '{}', 'not json',
                         '{"status":"Invalid"}\n{"status":"Accepted"}', '{"status":"Accepted"}']:
            with self.subTest(response=response):
                self.env["MOCK_NOTARY_JSON"] = response
                accepted = response == '{"status":"Accepted"}'
                self.run_script("sign-macos.sh", "20260920.3", app, self.work, success=accepted)
                self.assertEqual(output.exists(), accepted)
                self.assertEqual(update.exists(), accepted)
                self.assertFalse(list(self.work.glob(".herdr-sign.*")))
        with tarfile.open(update) as archive:
            self.assertEqual(archive.extractfile("Herdr.app/Contents/MacOS/Herdr").read(), b"never run")
            self.assertEqual(archive.extractfile("Herdr.app/Contents/_CodeSignature/CodeResources").read(), b"mock code signature")
            self.assertEqual(archive.extractfile("Herdr.app/Contents/CodeResources").read(), b"mock stapled ticket")
            self.assertEqual(archive.extractfile("Herdr.app/Contents/Resources/THIRD-PARTY-NOTICES.txt").read(), self.notices.read_bytes())
        self.assertFalse((app / "Contents/_CodeSignature").exists())
        self.assertFalse((app / "Contents/CodeResources").exists())
        output.unlink()
        # Existing updater outputs also fail before creating credentials.
        before = (self.work / "log").read_text()
        self.run_script("sign-macos.sh", "20260920.3", app, self.work, success=False)
        self.assertEqual((self.work / "log").read_text(), before)
        update.unlink()
        for tool in ["codesign", "spctl", "hdiutil", "xcrun"]:
            self.env["MOCK_FAIL"] = tool
            self.run_script("sign-macos.sh", "20260920.3", app, self.work, success=False)
            self.assertFalse(output.exists())
            self.assertFalse(update.exists())
            self.assertFalse(list(self.work.glob(".herdr-sign.*")))
        del self.env["MOCK_FAIL"]
        # A USTAR packaging failure must not promote the already signed DMG.
        (app / ("x" * 101)).write_bytes(b"unsupported USTAR name")
        self.run_script("sign-macos.sh", "20260920.3", app, self.work, success=False)
        self.assertFalse(output.exists())
        self.assertFalse(update.exists())
        self.assertFalse(list(self.work.glob(".herdr-sign.*")))
        calls = [json.loads(line) for line in (self.work / "log").read_text().splitlines()]
        restores = [c for c in calls if c[:5] == ["security", "list-keychains", "-d", "user", "-s"]]
        self.assertTrue(restores)
        originals = ["/mock/login keychain-db", "/mock/system.keychain"]
        # Setup must add the signing keychain, because codesign looks the identity
        # up through the search list rather than --keychain, but it must never drop
        # an existing keychain, and the final call must restore exactly the original
        # list so the machine is left as found even when signing fails.
        self.assertTrue(all(c[5 : 5 + len(originals)] == originals for c in restores))
        self.assertTrue(all(len(c[5:]) in (len(originals), len(originals) + 1) for c in restores))
        self.assertEqual(restores[-1][5:], originals)
        self.assertTrue(
            any(
                len(c[5:]) == len(originals) + 1 and c[-1].endswith("signing.keychain-db")
                for c in restores
            )
        )
        signs = [c for c in calls if c[:2] == ["codesign", "--force"]]
        self.assertTrue(any(c[-1].endswith("/Contents/MacOS/Herdr") for c in signs))
        self.assertTrue(any(c[-1].endswith("/Herdr.app") for c in signs))
        # The nested herdr-acp, herdr-remote-control, herdr-automate, and
        # herdr-core are signed before the app seals it, without the app's
        # entitlements.
        targets = [c[-1].rsplit("/", 1)[-1] for c in signs]
        for nested in ("herdr-acp", "herdr-remote-control", "herdr-automate", "herdr-core"):
            self.assertLess(targets.index(nested), targets.index("Herdr.app"))
            self.assertFalse(any("--entitlements" in c for c in signs if c[-1].endswith(f"/{nested}")))
        self.assertTrue(all("--timestamp" in c for c in signs))
        self.assertTrue(all("runtime" in c for c in signs if not c[-1].endswith(".dmg")))
        del self.env["APPLE_API_PRIVATE_KEY"]
        before = (self.work / "log").read_text()
        self.run_script("sign-macos.sh", "20260920.3", app, self.work, success=False)
        self.assertEqual((self.work / "log").read_text(), before)


if __name__ == "__main__":
    unittest.main()
