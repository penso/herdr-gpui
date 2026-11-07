"""Standalone alias safety, distro materialization, and Nix command aliases."""
import importlib.util
import io
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("linux_distro", ROOT / "scripts/release/package-linux-distro.py")
DISTRO = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DISTRO)


class LinuxAliasTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.work = Path(temporary.name)

    def archive(self, changes=None, duplicate=False):
        changes = changes or {}
        archive = self.work / "manual.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            for relative in sorted(DISTRO.TREE):
                entry = tarfile.TarInfo("release/" + relative)
                entry.mode = 0o755 if relative.startswith("bin/") else 0o644
                kind, link = changes.get(relative, (tarfile.SYMTYPE, "sheprd")
                                         if relative == "bin/herdr-gpui" else (tarfile.REGTYPE, ""))
                entry.type, entry.linkname = kind, link
                data = b"canonical executable" if relative == "bin/sheprd" else b"fixture"
                entry.size = len(data) if entry.isfile() else 0
                output.addfile(entry, io.BytesIO(data) if entry.isfile() else None)
                if duplicate and relative == "bin/herdr-gpui":
                    output.addfile(entry)
        return archive

    def test_distro_materializes_main_alias_as_a_regular_executable(self):
        root = DISTRO.extract(self.archive(), "release", self.work / "tree")
        canonical, legacy = root / "bin/sheprd", root / "bin/herdr-gpui"
        self.assertFalse(legacy.is_symlink())
        self.assertTrue(legacy.is_file())
        self.assertEqual(legacy.stat().st_mode & 0o777, 0o755)
        self.assertEqual(legacy.read_bytes(), canonical.read_bytes())
        config = DISTRO.config("20261103.1", "x86_64-unknown-linux-gnu", root, "2026-11-03T00:00:00Z")
        entry = next(entry for entry in config["contents"] if entry["dst"] == "/usr/bin/herdr-gpui")
        self.assertEqual(entry["type"], "file")
        self.assertEqual((entry["file_info"]["owner"], entry["file_info"]["group"]), ("root", "root"))

    def test_only_exact_relative_main_symlink_is_accepted(self):
        cases = [
            {"bin/herdr-gpui": (tarfile.SYMTYPE, target)}
            for target in ("/usr/bin/sheprd", "../bin/sheprd", "../../outside", "./sheprd",
                           "herdr-gpui", "sheprd-acp", "subdir/sheprd", "..\\sheprd")
        ]
        cases += [
            {"bin/herdr-gpui": (tarfile.LNKTYPE, "release/bin/sheprd")},
            {"bin/sheprd": (tarfile.SYMTYPE, "sheprd-acp")},
            {"bin/herdr-acp": (tarfile.SYMTYPE, "sheprd-acp")},
            {"bin/sheprd": (tarfile.DIRTYPE, "")},
        ]
        for changes in cases:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                DISTRO.extract(self.archive(changes), "release", self.work / "tree")
            self.assertFalse((self.work / "tree").exists())
        with self.assertRaisesRegex(ValueError, "Duplicate archive entry"):
            DISTRO.extract(self.archive(duplicate=True), "release", self.work / "tree")

    def test_nix_aliases_invoke_final_wrapper_and_owned_helpers(self):
        # Execute the actual postFixup shell with stand-ins for Nix's wrapper
        # and patchelf. The Linux Nix workflow also checks the built output.
        text = (ROOT / "flake.nix").read_text()
        hook = re.search(r"postFixup = ''\n(.*?)\n\s*'';", text, re.S)[1]
        hook = re.sub(r"\$\{lib\.[^}]+\}", "/fixture", hook)
        binaries = self.work / "bin"
        binaries.mkdir()
        commands = ("sheprd", "sheprd-acp", "sheprd-automate", "sheprd-nearby", "sheprd-remote-control")
        for command in commands:
            executable = binaries / command
            executable.write_text(f"#!/bin/sh\necho {command}\n")
            executable.chmod(0o755)
        core = binaries / "herdr-core"
        core.write_bytes(b"upstream unchanged")
        stubs = r'''
patchelf() { :; }
wrapProgram() {
    mv "$1" "$1.real"
    printf '#!/bin/sh\nprintf "wrapped:"\nexec "%s" "$@"\n' "$1.real" > "$1"
    chmod +x "$1"
}
'''
        subprocess.run(["bash", "-euc", stubs + hook], check=True,
                       env={**os.environ, "out": str(self.work)}, capture_output=True)
        aliases = {"herdr-gpui": "sheprd", **{f"herdr-{helper}": f"sheprd-{helper}"
                   for helper in ("acp", "automate", "nearby", "remote-control")}}
        for alias, canonical in aliases.items():
            with self.subTest(alias=alias):
                self.assertEqual(os.readlink(binaries / alias), canonical)
                output = subprocess.check_output([str(binaries / alias)], text=True).strip()
                self.assertEqual(output, "wrapped:sheprd" if canonical == "sheprd" else canonical)
        self.assertEqual(core.read_bytes(), b"upstream unchanged")
        self.assertFalse(core.is_symlink())
