"""site/install.ps1: its Start menu shortcut, taskbar identity, Explorer entry,
and herdr-gpui:// links, registered as the Setup.exe installer does."""
from pathlib import Path
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
INSTALL = ROOT / "site/install.ps1"
ISS = ROOT / "scripts/release/herdr-gpui.iss"
SCHEME_KEY = r"HKCU:\Software\Classes\herdr-gpui"
COMMAND_KEY = r"shell\open\command"

# Reads a link scheme's values back, and asks Windows which command opens it.
SCHEME_CHECK = r"""
Add-Type -Name Assoc -Namespace HerdrTest -MemberDefinition @'
[DllImport("shlwapi.dll", CharSet = CharSet.Unicode)]
public static extern int AssocQueryString(int flags, int str, string assoc, string extra,
  System.Text.StringBuilder output, ref int length);
'@
$output = New-Object System.Text.StringBuilder 1024
$length = $output.Capacity
# ASSOCF_IS_PROTOCOL, ASSOCSTR_COMMAND
$found = [HerdrTest.Assoc]::AssocQueryString(0x1000, 1, $env:HERDR_TEST_SCHEME, 'open', $output, [ref]$length)
$key = Get-Item -LiteralPath $scheme
$registered = [ordered]@{
  default = $key.GetValue('')
  protocol = $key.GetValue('URL Protocol')
  icon = (Get-Item -LiteralPath "$scheme\DefaultIcon").GetValue('')
  command = (Get-Item -LiteralPath "$scheme\shell\open\command").GetValue('')
  associated = $(if ($found -eq 0) { $output.ToString() } else { $null })
}
"""


def app_id():
    identity = (ROOT / "crates/herdr-updater/src/lib.rs").read_text(encoding="utf-8")
    return re.search(r'^pub const APP_ID: &str = "([^"]+)";$', identity, re.M).group(1)


def snippet():
    """The script's Add-Type block and the call that sets the shortcut's ID."""
    source = INSTALL.read_text(encoding="utf-8")
    start = source.index("    Add-Type -TypeDefinition")
    end = source.index("\n", source.index("[HerdrShortcut]::SetAppUserModelId(", start))
    return source[start:end]


def install_steps():
    """Everything the script does once the release's files are in place."""
    source = INSTALL.read_text(encoding="utf-8")
    start = source.index("  $hasLauncher = Test-Path")
    end = source.index("\n  }\n", source.index("Start it from the Start menu (Herdr UI) or with herdr-gpui.", start))
    return source[start:end + len("\n  }")]


def scheme_lines():
    """The script's lines that register herdr-gpui:// links, and the ones
    that remove them: -Uninstall's first, then a launcher-less release's."""
    lines = INSTALL.read_text(encoding="utf-8").splitlines()
    register = [line for line in lines
                if re.match(r' +(New-Item -Force -Path|Set-Item|Set-ItemProperty) [^$]*"?\$scheme\b', line)]
    remove = [line for line in lines if line.lstrip().startswith("Remove-Item") and line.endswith(" $scheme")]
    return register, remove


def installer_scheme_values(app):
    """What Setup.exe writes under the scheme's key, keyed by (subkey below
    it, value name), with its install directory set to `app`."""
    iss = ISS.read_text(encoding="utf-8")
    exe = re.search(r'^#define AppExe "([^"]+)"$', iss, re.M).group(1)
    launcher = re.search(r'^#define Launcher "([^"]+)"$', iss, re.M).group(1)
    pattern = (r'^Root: HKCU; Subkey: "Software\\Classes\\herdr-gpui(?:\\([^"]+))?"; ValueType: string; '
               r'ValueName: "([^"]*)"; ValueData: "((?:[^"]|"")*)"')
    values = {}
    for subkey, name, data in re.findall(pattern, iss, re.M):
        data = data.replace('""', '"').replace("{app}", app)
        values[(subkey, name)] = data.replace("{#AppExe}", exe).replace("{#Launcher}", launcher)
    return values


class InstallScriptTests(unittest.TestCase):
    def test_shortcut_gets_the_apps_and_installers_user_model_id(self):
        install = INSTALL.read_text(encoding="utf-8")
        self.assertIn(f"[HerdrShortcut]::SetAppUserModelId($shortcut, '{app_id()}')", install)
        iss = ISS.read_text(encoding="utf-8")
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

    def test_links_are_registered_on_install_and_removed_on_uninstall(self):
        install = INSTALL.read_text(encoding="utf-8")
        self.assertIn(f"  $scheme = '{SCHEME_KEY}'\n", install)
        self.assertLess(install.index("  $scheme = "), install.index("  if ($Uninstall) {"))
        register, remove = scheme_lines()
        self.assertEqual(len(register), 6)
        self.assertEqual(len(remove), 2)
        steps = install_steps()
        self.assertTrue(all(line in steps for line in register), register)
        self.assertLess(install.index(remove[0]), install.index("    Write-Host \"Uninstalled Herdr UI from"))
        self.assertIn(remove[1], steps)

    def test_installed_automation_daemon_stops_before_its_files_change(self):
        # Windows will not overwrite or delete herdr-automate.exe while the
        # daemon the app started from it still runs.
        install = INSTALL.read_text(encoding="utf-8")
        stop = install.index("  $stopAutomate = {")
        self.assertIn("$_.Path -eq $automate", install[stop:install.index("\n  }\n", stop)])
        self.assertLess(stop, install.index("  if ($Uninstall) {"))
        self.assertLess(install.index("    & $stopAutomate | Out-Null"),
                        install.index("    if (Test-Path -LiteralPath $dest) { Remove-Item"))
        self.assertLess(install.index("    $restartAutomate = & $stopAutomate"),
                        install.index("    Copy-Item -Recurse -Force"))
        self.assertLess(install.index("    Copy-Item -Recurse -Force"),
                        install.index("    & $automate start"))
        iss = ISS.read_text(encoding="utf-8")
        for caller in ("function PrepareToInstall(", "function InitializeUninstall("):
            body = iss[iss.index(caller):]
            self.assertIn("StopAutomate()", body[:body.index("\nend;")], caller)
        self.assertIn('Parameters: "start"; WorkingDir: "{app}"; Flags: runhidden nowait; Check: AutomateStopped', iss)

    @unittest.skipUnless(sys.platform == "win32" and shutil.which("powershell.exe"), "needs Windows PowerShell")
    def test_links_open_the_launcher_as_setup_exe_registers_and_uninstall_removes_them(self):
        """Registers a throwaway scheme with the script's own lines, reads it
        back and asks Windows which command opens it, then removes it with
        the script's uninstall line."""
        register, remove = scheme_lines()
        app = r"C:\Program Files Test\Herdr UI"
        expected = installer_scheme_values(app)
        self.assertEqual(set(expected), {("", ""), ("", "URL Protocol"), ("DefaultIcon", ""), (COMMAND_KEY, "")})
        scheme = f"herdr-gpui-test-{os.getpid()}"
        with tempfile.TemporaryDirectory(prefix="herdr-install-ps1-") as temp:
            script = Path(temp) / "check.ps1"
            script.write_text(
                "$ErrorActionPreference = 'Stop'\n"
                "$exe = Join-Path $env:HERDR_TEST_APP 'herdr-gpui.exe'\n"
                "$launcher = Join-Path $env:HERDR_TEST_APP 'herdr-gpui-launcher.exe'\n"
                "$scheme = 'HKCU:\\Software\\Classes\\' + $env:HERDR_TEST_SCHEME\n"
                "try {\n" + "\n".join(register) + SCHEME_CHECK
                + "} finally {\n" + remove[0] + "\n}\n"
                "$registered.removed = -not (Test-Path -LiteralPath $scheme)\n"
                "$registered | ConvertTo-Json\n",
                encoding="utf-8")
            result = subprocess.run(
                ["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                 "-File", str(script)],
                capture_output=True, text=True, timeout=120,
                env={**os.environ, "HERDR_TEST_APP": app, "HERDR_TEST_SCHEME": scheme})
        self.assertEqual(result.returncode, 0, result.stderr)
        values = json.loads(result.stdout)
        self.assertEqual(values["default"], expected[("", "")])
        self.assertEqual(values["protocol"], expected[("", "URL Protocol")])
        self.assertEqual(values["icon"], expected[("DefaultIcon", "")])
        self.assertEqual(values["command"], expected[(COMMAND_KEY, "")])
        self.assertEqual(values["command"], f'"{app}\\herdr-gpui-launcher.exe" "%1"')
        self.assertEqual(values["associated"], values["command"])
        self.assertTrue(values["removed"])

    @unittest.skipUnless(sys.platform == "win32" and shutil.which("powershell.exe"), "needs Windows PowerShell")
    def test_shortcut_explorer_entry_and_links_follow_the_releases_launcher(self):
        # Releases up to 20261016.1 ship herdr-gpui.exe alone (#632): there
        # the shortcut opens it, an existing Explorer entry and link
        # registration go, and the closing message names neither the
        # launcher nor that entry.
        for has_launcher in (False, True):
            key = rf"Software\HerdrInstallPs1Test-{os.getpid()}-{has_launcher}"
            with self.subTest(has_launcher=has_launcher), \
                    tempfile.TemporaryDirectory(prefix="herdr-install-ps1-") as temp:
                dest = Path(temp) / "Herdr UI"
                dest.mkdir()
                launcher = dest / "herdr-gpui-launcher.exe"
                (dest / "herdr-gpui.exe").write_bytes(b"")
                if has_launcher:
                    launcher.write_bytes(b"")
                script = Path(temp) / "check.ps1"
                script.write_text("\n".join([
                    "$ErrorActionPreference = 'Stop'",
                    "$version = '20000101.1'",
                    "$dest = $env:HERDR_TEST_DEST",
                    "$exe = Join-Path $dest 'herdr-gpui.exe'",
                    "$launcher = Join-Path $dest 'herdr-gpui-launcher.exe'",
                    rf"$verbs = 'HKCU:\{key}\Directory', 'HKCU:\{key}\Background'",
                    rf"$scheme = 'HKCU:\{key}\Scheme'",
                    # An entry and a link registration left from an earlier install.
                    r'foreach ($verb in $verbs) { New-Item -Force -Path "$verb\command" | Out-Null }',
                    r'New-Item -Force -Path "$scheme\shell\open\command" | Out-Null',
                    install_steps().replace("[Environment]::GetFolderPath('Programs')", "$env:HERDR_TEST_DIR"),
                    "'target=' + (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut).TargetPath",
                    "foreach ($verb in $verbs) {",
                    r"""  if (Test-Path -LiteralPath $verb) { 'verb=' + (Get-Item -LiteralPath "$verb\command").GetValue('') }""",
                    "}",
                    r"""if (Test-Path -LiteralPath $scheme) { 'scheme=' + (Get-Item -LiteralPath "$scheme\shell\open\command").GetValue('') }""",
                    "",
                ]), encoding="utf-8")
                try:
                    result = subprocess.run(
                        ["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                         "-File", str(script)],
                        capture_output=True, text=True, timeout=120,
                        env={**os.environ, "HERDR_TEST_DIR": temp, "HERDR_TEST_DEST": str(dest)})
                finally:
                    subprocess.run(["reg.exe", "delete", rf"HKCU\{key}", "/f"], capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                lines = result.stdout.splitlines()
                message = next(line for line in lines if line.startswith("Installed Herdr UI"))
                target = next(line for line in lines if line.startswith("target="))[len("target="):]
                verbs = [line[len("verb="):] for line in lines if line.startswith("verb=")]
                schemes = [line[len("scheme="):] for line in lines if line.startswith("scheme=")]
                # samefile, since Windows can hand back the temp directory
                # as an 8.3 short path.
                self.assertTrue(os.path.samefile(target, launcher if has_launcher else dest / "herdr-gpui.exe"),
                                target)
                if has_launcher:
                    self.assertEqual(len(verbs), 2, result.stdout)
                    for verb in verbs:
                        program, rest = verb[1:].split('"', 1)
                        self.assertTrue(os.path.samefile(program, launcher), verb)
                        self.assertEqual(rest, ' open "%V"')
                    self.assertEqual(len(schemes), 1, result.stdout)
                    program, rest = schemes[0][1:].split('"', 1)
                    self.assertTrue(os.path.samefile(program, launcher), schemes[0])
                    self.assertEqual(rest, ' "%1"')
                    self.assertIn("herdr-gpui-launcher.exe", message)
                    self.assertIn("Open in Herdr UI", message)
                else:
                    self.assertEqual(verbs, [], result.stdout)
                    self.assertEqual(schemes, [], result.stdout)
                    self.assertNotIn("launcher", message)
                    self.assertNotIn("Open in Herdr UI", message)


if __name__ == "__main__":
    unittest.main()
