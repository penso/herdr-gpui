# Windows QA prompt

A prompt for a coding agent running on a Windows machine with this repository
checked out, to test the launcher, the command line, and the installer.
Copy everything in the block below into the agent.

```text
You're on a Windows machine with the herdr-gpui repo checked out. Test the `windows` branch,
which changes how Herdr UI meets the Windows console and adds an installer. Read AGENTS.md first,
and don't modify or push anything unless I ask; report what you find.

What the branch does:
- herdr-gpui.exe is a console program, because it is also the command line: shells wait for it
  and see its exit code. herdr-gpui-launcher.exe, a windowed program beside it, starts it without
  a console window; the Start menu shortcut opens the launcher.
- Every child process (git, ssh, wsl.exe…) is created with CREATE_NO_WINDOW via
  herdr_client::new_command, so none flash a console.
- scripts/release/herdr-gpui.iss is a per-user Inno Setup installer; scripts/release/windows-installer.py
  builds and smoke-tests it. It shows Herdr's icon, and its shortcuts open the launcher and carry
  the app's AppUserModelID (so.pen.herdr-gpui).
- herdr-gpui.exe embeds Herdr's icon and a version resource (FileDescription "Herdr UI"), and each
  window carries the AppUserModelID with relaunch properties naming the launcher, so the window
  and a pinned shortcut share one taskbar button.

Steps (PowerShell, from the repo root):
1. git fetch origin; git checkout windows; git pull
2. $env:HERDR_RELEASE_VERSION = '20261015.1'; cargo build --release -p herdr-gpui -p herdr-gpui-launcher
   (Get-Item target\release\herdr-gpui.exe).VersionInfo | Format-List FileDescription, ProductName, FileVersion
   (expect Herdr UI, Herdr UI, 20261015.1)
3. Remove-Item -Recurse -Force dist -ErrorAction SilentlyContinue; mkdir dist
   python scripts\release\package-windows.py 20261015.1 x86_64-pc-windows-msvc target\release\herdr-gpui.exe target\release\herdr-gpui-launcher.exe dist LICENSE
4. Find ISCC.exe (Inno Setup 6 is installed; try "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
   and "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"), then:
   python scripts\release\windows-installer.py build 20261015.1 x86_64-pc-windows-msvc dist\Herdr-20261015.1-x86_64-pc-windows-msvc.zip dist <ISCC path>
5. python scripts\release\windows-installer.py smoke dist\Herdr-20261015.1-x86_64-pc-windows-msvc-setup.exe
   (silent install to a temp dir, checks files, subsystems, output, shortcut and PATH, then uninstalls)
   Run it from an ordinary terminal: inside a packaged (MSIX) app such as the Claude desktop app,
   Windows redirects the installer's registry writes into that app's private hive.

Then, using computer use if you have it (otherwise give me exact steps and I'll do them):
6. Run the setup exe normally. A signed release names its publisher in any
   SmartScreen prompt; a local build is unsigned and needs More info → Run
   anyway. Keep "Add herdr-gpui to PATH". Afterwards, right-click a folder, and the empty space
   inside one: Open in Herdr UI is under Show more options on Windows 11. Choose it with the app
   closed, then again with it open: the folder opens as a workspace, no console window appears,
   and the second time the existing workspace is focused rather than a new one made.
7. Open "Herdr UI" from the Start menu. Take screenshots right away: does any console window appear,
   even briefly? Use the app a bit (open a repo/worktree so git runs) and watch for console flashes.
8. In a NEW PowerShell window: `herdr-gpui --help` (output prints, prompt returns after it) and
   `herdr-gpui --build-info; $LASTEXITCODE` (expect 0).
   Check that Task Manager lists "Herdr UI" with the ram icon, and that Explorer shows the same icon
   for the installed herdr-gpui.exe. Pin the running app to the taskbar, quit it, start it from the
   pin, and trigger a system notification (Settings: system notifications on, then an agent that
   needs attention): the window, the pin, and the notification must all use one taskbar button.
9. Uninstall from Settings → Apps → Herdr UI; confirm %LOCALAPPDATA%\Programs\Herdr UI is gone and
   `herdr-gpui` is no longer found in a new terminal, and Open in Herdr UI is gone from
   folders' menus.

Report: Windows version and architecture, the output of each command (full error text for any
failure, including ISCC line numbers), and for steps 6–9 what you saw, with screenshots.
```

On an ARM64 machine, replace `x86_64-pc-windows-msvc` with `aarch64-pc-windows-msvc`
throughout.
