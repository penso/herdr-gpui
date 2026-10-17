"""site/install.ps1 gives its Start menu shortcut the app's AppUserModelID."""
from pathlib import Path
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
INSTALL = ROOT / "site/install.ps1"


def app_id():
    constants = (ROOT / "crates/herdr-gpui/src/constants.rs").read_text(encoding="utf-8")
    return re.search(r'^pub\(crate\) const APP_ID: &str = "([^"]+)";$', constants, re.M).group(1)


def snippet():
    """The script's Add-Type block and the call that sets the shortcut's ID."""
    source = INSTALL.read_text(encoding="utf-8")
    start = source.index("    Add-Type -TypeDefinition")
    end = source.index("\n", source.index("[HerdrShortcut]::SetAppUserModelId(", start))
    return source[start:end]


class InstallScriptTests(unittest.TestCase):
    def test_shortcut_gets_the_apps_and_installers_user_model_id(self):
        install = INSTALL.read_text(encoding="utf-8")
        self.assertIn(f"[HerdrShortcut]::SetAppUserModelId($shortcut, '{app_id()}')", install)
        iss = (ROOT / "scripts/release/herdr-gpui.iss").read_text(encoding="utf-8")
        self.assertIn(f'#define AppUserModelID "{app_id()}"\n', iss)

    @unittest.skipUnless(sys.platform == "win32" and shutil.which("powershell.exe"), "needs Windows PowerShell")
    def test_snippet_writes_the_id_windows_reads_back(self):
        with tempfile.TemporaryDirectory(prefix="herdr-install-ps1-") as temp:
            script = Path(temp) / "check.ps1"
            script.write_text(
                "$ErrorActionPreference = 'Stop'\n"
                "$shortcut = Join-Path $env:HERDR_TEST_DIR 'Herdr UI.lnk'\n"
                "$link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)\n"
                "$link.TargetPath = Join-Path $env:SystemRoot 'notepad.exe'\n"
                "$link.Save()\n"
                + snippet() + "\n"
                "(New-Object -ComObject Shell.Application).Namespace($env:HERDR_TEST_DIR)"
                ".ParseName('Herdr UI.lnk').ExtendedProperty('System.AppUserModel.ID')\n",
                encoding="utf-8")
            result = subprocess.run(
                ["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                 "-File", str(script)],
                capture_output=True, text=True, timeout=120,
                env={**os.environ, "HERDR_TEST_DIR": temp})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), app_id())


if __name__ == "__main__":
    unittest.main()
