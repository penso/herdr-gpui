# Installs the latest stable Herdr UI release for the current user on Windows,
# served at https://herdrui.dev/install.ps1:
#
#   irm https://herdrui.dev/install.ps1 | iex
#
# It downloads the zip for this machine from GitHub Releases, checks it against
# the release's published SHA-256, installs herdr-gpui.exe,
# herdr-gpui-launcher.exe, herdr-acp.exe, the remote control daemon
# herdr-remote-control.exe, and the automation daemon herdr-automate.exe
# under %LOCALAPPDATA%\Programs\Herdr UI, adds that
# directory to the user PATH so herdr-gpui works as a command, and creates a
# Start menu shortcut to herdr-gpui-launcher.exe, the windowed launcher that
# starts herdr-gpui.exe without a console window. It never installs, starts or
# upgrades the Herdr daemon. Signed copies update themselves from Check for
# Updates in the app; running this again also updates. It also adds "Open in Herdr UI" to
# folders in File Explorer and registers herdr-gpui:// links, for the current
# user only.
#
# To uninstall, run it with -Uninstall:
#
#   & ([scriptblock]::Create((irm https://herdrui.dev/install.ps1))) -Uninstall
#
# That removes what this script added: the files, the PATH entry, the Start
# menu shortcut, the Explorer entry, and the link registration. Settings and
# Herdr sessions are kept.
& {
  param([switch]$Uninstall)
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

  $dest = Join-Path $env:LOCALAPPDATA 'Programs\Herdr UI'
  $exe = Join-Path $dest 'herdr-gpui.exe'
  # The launcher exits as soon as herdr-gpui.exe starts, so only that one can
  # be running during an update.
  $launcher = Join-Path $dest 'herdr-gpui-launcher.exe'
  # Windows will not overwrite a running executable, so an update needs the
  # installed app closed. Check before downloading and again before copying,
  # and before uninstalling.
  $assertNotRunning = {
    $running = Get-Process -Name 'herdr-gpui' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }
    if ($running) { throw "Herdr UI is running from $exe. Quit it, then run this command again; your Herdr sessions keep running." }
  }
  & $assertNotRunning
  # The automation daemon the app starts keeps running after it quits, and
  # Windows will not overwrite herdr-automate.exe while it runs either. Ask
  # the installed copy to stop once its runs finish, wait for it to exit, and
  # say whether it was running so an update can start it again. A daemon
  # started from anywhere else is left alone.
  $automate = Join-Path $dest 'herdr-automate.exe'
  $stopAutomate = {
    $running = { Get-Process -Name 'herdr-automate' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $automate } }
    if (-not (& $running)) { return $false }
    Write-Host 'Stopping the automation daemon once its runs finish'
    & $automate shutdown | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "The automation daemon at $automate did not stop. Stop its runs with herdr-automate stop, then run this command again." }
    $deadline = (Get-Date).AddSeconds(30)
    while ((& $running) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200 }
    if (& $running) { throw "The automation daemon at $automate is still running. Quit it, then run this command again." }
    return $true
  }

  # "Open in Herdr UI" on a folder, and on the background inside one: the
  # launcher runs `herdr-gpui open`, which opens the folder as Open Project
  # does and starts the app first when it is not running. Windows 11 lists it
  # under "Show more options". The Setup.exe installer writes the same keys.
  $verbs = 'HKCU:\Software\Classes\Directory\shell\HerdrUI', 'HKCU:\Software\Classes\Directory\Background\shell\HerdrUI'
  # herdr-gpui:// links (jump list entries, notification clicks, links from
  # other apps) start the launcher with the link as its only argument, and
  # herdr-gpui.exe hands it to the running app. Setup.exe writes the same keys.
  $scheme = 'HKCU:\Software\Classes\herdr-gpui'

  if ($Uninstall) {
    # Over a Setup.exe installation its own uninstaller removes everything,
    # including its entry in Settings.
    $uninstaller = Join-Path $dest 'unins000.exe'
    if (Test-Path -LiteralPath $uninstaller) {
      $process = Start-Process -Wait -PassThru -FilePath $uninstaller -ArgumentList '/SILENT'
      if ($process.ExitCode -ne 0) { throw "The Herdr UI uninstaller exited with code $($process.ExitCode); Herdr UI may still be installed in $dest." }
      # The uninstaller hands off to a copy of itself and returns early, so
      # wait for it to remove the app.
      $deadline = (Get-Date).AddMinutes(2)
      while ((Test-Path -LiteralPath $exe) -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 1 }
      if (Test-Path -LiteralPath $exe) { throw "The Herdr UI uninstaller left $exe in place." }
      Write-Host 'Uninstalled Herdr UI.'
      return
    }
    foreach ($verb in $verbs) { Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $verb }
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $scheme
    Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath (Join-Path ([Environment]::GetFolderPath('Programs')) 'Herdr UI.lnk')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $kept = @($userPath -split ';' | Where-Object { $_ -and $_.TrimEnd([char]92) -ne $dest })
    if ($kept.Count -ne @($userPath -split ';' | Where-Object { $_ }).Count) {
      [Environment]::SetEnvironmentVariable('Path', $kept -join ';', 'User')
    }
    & $stopAutomate | Out-Null
    if (Test-Path -LiteralPath $dest) { Remove-Item -Recurse -Force -LiteralPath $dest }
    if (Test-Path -LiteralPath $dest) { throw "Could not remove $dest." }
    Write-Host "Uninstalled Herdr UI from $dest. Settings and Herdr sessions were kept."
    return
  }

  $repo = 'penso/herdr-gpui'
  $os = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture
  $arch = switch ($os) {
    'X64' { 'x86_64' }
    'Arm64' { 'aarch64' }
    default { throw "Herdr UI has no Windows build for $os" }
  }

  # The latest release is always the newest stable one; betas are prereleases.
  $tag = (Invoke-RestMethod -UseBasicParsing "https://api.github.com/repos/$repo/releases/latest").tag_name
  if ($tag -notmatch '^v([1-9][0-9]{7}\.[1-9][0-9]*)$') { throw "Unexpected latest release tag: $tag" }
  $version = $Matches[1]

  $name = "Herdr-$version-$arch-pc-windows-msvc"
  $base = "https://github.com/$repo/releases/download/$tag"
  $tmp = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
  New-Item -ItemType Directory -Path $tmp | Out-Null
  try {
    Write-Host "Downloading $name.zip"
    $zip = Join-Path $tmp "$name.zip"
    Invoke-WebRequest -UseBasicParsing "$base/$name.zip" -OutFile $zip
    $sums = Join-Path $tmp "$name.zip.sha256"
    Invoke-WebRequest -UseBasicParsing "$base/$name.zip.sha256" -OutFile $sums
    $expected = ((Get-Content -Raw $sums).Trim() -split '\s+')[0]
    $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash
    if ($actual -ne $expected) { throw "Checksum mismatch for $name.zip" }

    Expand-Archive -Path $zip -DestinationPath $tmp
    & $assertNotRunning
    $restartAutomate = & $stopAutomate
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    Copy-Item -Recurse -Force -Path (Join-Path $tmp "$name\*") -Destination $dest
  } finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
  }

  # The new copy, as Herdr UI would start it for the scripts it lists.
  if ($restartAutomate) {
    & $automate start | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Warning 'Could not start the automation daemon again; Herdr UI starts it when it next opens.' }
  }

  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not (($userPath -split ';') -contains $dest)) {
    [Environment]::SetEnvironmentVariable('Path', (@($userPath, $dest) | Where-Object { $_ }) -join ';', 'User')
    Write-Host "Added $dest to your user PATH; open a new terminal to use herdr-gpui."
  }

  # Releases up to 20261016.1 ship no launcher, and this script is served
  # ahead of the release it describes. Point the shortcut at herdr-gpui.exe
  # then, which opens a console window beside the app, rather than at nothing.
  $hasLauncher = Test-Path -LiteralPath $launcher
  $target = if ($hasLauncher) { $launcher } else { $exe }
  $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Herdr UI.lnk'
  $link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
  $link.TargetPath = $target
  $link.WorkingDirectory = $dest
  $link.Save()
  # Each Herdr window carries this AppUserModelID (constants::APP_ID, see
  # taskbar.rs). Without the same one on the shortcut, the app opened or
  # pinned from it groups apart from its windows. WScript.Shell cannot set
  # it, so write the shortcut's property store. Without it the app still
  # works, so a failure only warns.
  try {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class HerdrShortcut {
  [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  interface IPropertyStore {
    void GetCount(out uint count);
    void GetAt(uint index, out PropertyKey key);
    void GetValue(ref PropertyKey key, out PropVariant value);
    void SetValue(ref PropertyKey key, ref PropVariant value);
    void Commit();
  }
  [StructLayout(LayoutKind.Sequential)]
  struct PropertyKey { public Guid Format; public uint Id; }
  // PROPVARIANT: a type tag, three reserved words, and a pointer-sized union.
  [StructLayout(LayoutKind.Sequential)]
  struct PropVariant { public ushort Type; ushort R1, R2, R3; public IntPtr Value; IntPtr Padding; }
  [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
  static extern void SHGetPropertyStoreFromParsingName(string path, IntPtr bindContext, int flags, ref Guid iid, out IPropertyStore store);
  public static void SetAppUserModelId(string path, string id) {
    Guid iid = typeof(IPropertyStore).GUID;
    IPropertyStore store;
    SHGetPropertyStoreFromParsingName(path, IntPtr.Zero, 2 /* GPS_READWRITE */, ref iid, out store);
    try {
      // PKEY_AppUserModel_ID, a VT_LPWSTR.
      PropertyKey key = new PropertyKey { Format = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), Id = 5 };
      PropVariant value = new PropVariant { Type = 31, Value = Marshal.StringToCoTaskMemUni(id) };
      try {
        store.SetValue(ref key, ref value);
        store.Commit();
      } finally {
        Marshal.FreeCoTaskMem(value.Value);
      }
    } finally {
      Marshal.ReleaseComObject(store);
    }
  }
}
'@
    [HerdrShortcut]::SetAppUserModelId($shortcut, 'so.pen.herdr-gpui')
  } catch {
    Write-Warning "Could not give the Start menu shortcut Herdr UI's taskbar identity: $_"
  }

  # A release without the launcher has no `herdr-gpui open` either, so it
  # gets no Explorer entry.
  foreach ($verb in $verbs) {
    if (-not $hasLauncher) {
      Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $verb
      continue
    }
    New-Item -Force -Path "$verb\command" | Out-Null
    Set-Item -LiteralPath $verb -Value 'Open in Herdr UI'
    Set-ItemProperty -LiteralPath $verb -Name 'Icon' -Value "`"$exe`",0"
    Set-Item -LiteralPath "$verb\command" -Value "`"$launcher`" open `"%V`""
  }

  # Links open through the launcher as well, so a release without it
  # registers none, and loses one an earlier install left.
  if ($hasLauncher) {
    New-Item -Force -Path "$scheme\shell\open\command" | Out-Null
    New-Item -Force -Path "$scheme\DefaultIcon" | Out-Null
    Set-Item -LiteralPath $scheme -Value 'URL:Herdr UI link'
    Set-ItemProperty -LiteralPath $scheme -Name 'URL Protocol' -Value ''
    Set-Item -LiteralPath "$scheme\DefaultIcon" -Value "`"$exe`",0"
    Set-Item -LiteralPath "$scheme\shell\open\command" -Value "`"$launcher`" `"%1`""
  } else {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $scheme
  }

  if ($hasLauncher) {
    Write-Host "Installed Herdr UI $version to $dest. Start it from the Start menu (Herdr UI), by running herdr-gpui-launcher.exe, or from Open in Herdr UI on a folder in File Explorer; use herdr-gpui for the command line."
  } else {
    Write-Host "Installed Herdr UI $version to $dest. Start it from the Start menu (Herdr UI) or with herdr-gpui."
  }
  if (-not (Get-Command herdr -ErrorAction SilentlyContinue)) {
    Write-Host 'Herdr UI needs the Herdr daemon, which is not on PATH. Install it from https://herdr.dev/'
  }
} @args
