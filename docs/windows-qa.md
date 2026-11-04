# Windows QA prompt

A prompt for a coding agent running on a Windows machine with this repository
checked out, to test the launcher, the command line, and the installer.
Copy everything in the block below into the agent.

```text
You're on a Windows machine with the sheprd repo checked out. Test the agreed rename revision,
including the console, launcher and installer compatibility. Read AGENTS.md first,
and don't modify or push anything unless I ask; report what you find.

What to verify:
- sheprd.exe is a console program, because it is also the command line: shells wait for it
  and see its exit code. sheprd-launcher.exe, a windowed program beside it, starts it without
  a console window; the Start menu shortcut opens the launcher.
- Every child process (git, ssh, wsl.exe…) is created with CREATE_NO_WINDOW via
  sheprd_client::new_command, so none flash a console.
- scripts/release/sheprd.iss is a per-user Inno Setup installer; scripts/release/windows-installer.py
  builds and smoke-tests it. It shows Sheprd's paw icon, and its shortcuts open the launcher and carry
  the app's AppUserModelID (so.pen.herdr-gpui).
- sheprd.exe embeds Sheprd's paw icon and a version resource (FileDescription "Sheprd"), and each
  window carries the AppUserModelID with relaunch properties naming the launcher, so the window
  and a pinned shortcut share one taskbar button.
- The installed directory stays Programs\Herdr UI, AppId stays
  {7FA70894-4FC0-4923-BDCB-B05F7475610A}, and winget stays Penso.HerdrUI.
  herdr-gpui.exe and herdr-gpui-launcher.exe remain alongside the canonical pair.

Steps (PowerShell, from the repo root):
1. Record git status and git rev-parse HEAD; preserve unrelated changes.
2. $env:HERDR_RELEASE_VERSION = '20261015.1'; cargo build --locked --release -p sheprd-gpui -p sheprd-launcher
   (Get-Item target\release\sheprd.exe).VersionInfo | Format-List FileDescription, ProductName, FileVersion
   (expect Sheprd, Sheprd, 20261015.1)
3. Use a fresh output directory (dist below); do not remove existing user output.
   python scripts\release\package-windows.py 20261015.1 x86_64-pc-windows-msvc target\release\sheprd.exe target\release\sheprd-launcher.exe dist LICENSE
4. Find ISCC.exe (Inno Setup 6 is installed; try "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
   and "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"), then:
   python scripts\release\windows-installer.py build 20261015.1 x86_64-pc-windows-msvc dist\Sheprd-20261015.1-x86_64-pc-windows-msvc.zip dist <ISCC path>
5. python scripts\release\windows-installer.py smoke dist\Sheprd-20261015.1-x86_64-pc-windows-msvc-setup.exe
   (silent install to a temp dir, checks files, subsystems, output, shortcut and PATH, then uninstalls)
   Run it from an ordinary terminal: inside a packaged (MSIX) app such as the Claude desktop app,
   Windows redirects the installer's registry writes into that app's private hive.

Then, using computer use if you have it (otherwise give me exact steps and I'll do them):
6. Run the setup exe normally. A signed release names its publisher in any
   SmartScreen prompt; a local build is unsigned and needs More info → Run
   anyway. Keep "Add sheprd to PATH". Afterwards, right-click a folder, and the empty space
   inside one: Open in Sheprd is under Show more options on Windows 11. Choose it with the app
   closed, then again with it open: the folder opens as a workspace, no console window appears,
   and the second time the existing workspace is focused rather than a new one made.
7. Open "Sheprd" from the Start menu. Take screenshots right away: does any console window appear,
   even briefly? Use the app a bit (open a repo/worktree so git runs) and watch for console flashes.
8. In a NEW PowerShell window: `sheprd --help` (output prints, prompt returns after it) and
   `sheprd --build-info; $LASTEXITCODE` (expect 0). Repeat with legacy herdr-gpui.exe.
   Check that Task Manager lists "Sheprd" with the paw icon, and that Explorer shows the same icon
   for the installed sheprd.exe. Pin the running app to the taskbar, quit it, start it from the
   pin, and trigger a system notification (Settings: system notifications on, then an agent that
   needs attention): the window, the pin, and the notification must all use one taskbar button.
9. Uninstall from Settings → Apps → Sheprd; confirm %LOCALAPPDATA%\Programs\Herdr UI is gone and
   neither `sheprd` nor `herdr-gpui` is found in a new terminal, and Open in Sheprd is gone from
   folders' menus.
10. Separately test a signed bridge-to-Sheprd upgrade with existing pins, saved credentials,
    sessions and notifications. Confirm the same install entry, restart and winget version.
    Local unsigned smoke tests do not establish this; use the rollout checklist in docs/rebranding.md.

Report: Windows version and architecture, the output of each command (full error text for any
failure, including ISCC line numbers), and for steps 6–10 what you saw, with screenshots.
```

On an ARM64 machine, replace `x86_64-pc-windows-msvc` with `aarch64-pc-windows-msvc`
throughout.
