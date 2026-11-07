# Installs the latest stable Sheprd release for the current user on Windows,
# served at https://sheprd.org/install.ps1:
#
#   irm https://sheprd.org/install.ps1 | iex
#
# It downloads the zip for this machine from GitHub Releases, checks it against
# the release's published SHA-256, and installs sheprd.exe, sheprd-launcher.exe,
# sheprd-acp.exe, sheprd-remote-control.exe, sheprd-automate.exe, sheprd-nearby.exe,
# and upstream herdr-core.exe, with legacy executable aliases. The directory
# %LOCALAPPDATA%\Programs\Herdr UI and AppUserModelID so.pen.herdr-gpui remain
# unchanged for compatibility. It adds the directory to the user PATH and a
# Sheprd Start menu shortcut. It never starts or stops terminal daemons, changes
# services or settings, or upgrades a personal Herdr installation. Signed copies
# update through Check for Updates; rerunning this also updates. Explorer's
# "Open in Sheprd" entry and both sheprd:// and herdr-gpui:// links are per-user.
#
# To uninstall, run it with -Uninstall:
#
#   & ([scriptblock]::Create((irm https://sheprd.org/install.ps1))) -Uninstall
#
# That removes what this script added: the files, the PATH entry, the Start
# menu shortcut, the Explorer entry, and the link registration. Settings and
# daemon sessions are kept.
& {
  param([switch]$Uninstall)
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

  $dest = Join-Path $env:LOCALAPPDATA 'Programs\Herdr UI'
  $exe = Join-Path $dest 'sheprd.exe'
  $legacyExe = Join-Path $dest 'herdr-gpui.exe'
  $launcher = Join-Path $dest 'sheprd-launcher.exe'
  $legacyLauncher = Join-Path $dest 'herdr-gpui-launcher.exe'
  $appPaths = @($exe, $legacyExe, $launcher, $legacyLauncher)
  # Windows will not overwrite a running executable, so an update needs the
  # installed app closed. Check before downloading and again before copying,
  # and before uninstalling.
  $assertNotRunning = {
    # Only exact installed paths are relevant. Never kill by process name or
    # touch a daemon/helper from another checkout or installation.
    $names = @('sheprd', 'sheprd-launcher', 'herdr-gpui', 'herdr-gpui-launcher',
      'sheprd-acp', 'herdr-acp', 'sheprd-nearby', 'herdr-nearby',
      'sheprd-remote-control', 'herdr-remote-control', 'herdr-core')
    foreach ($name in $names) {
      $path = Join-Path $dest "$name.exe"
      $running = Get-Process -Name $name -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $path }
      if ($running) { throw "Sheprd cannot replace or remove the running file $path. Close its app or active sessions before retrying. No terminal daemon was stopped." }
    }
  }
  & $assertNotRunning
  # The automation daemon the app starts keeps running after it quits, and
  # Windows will not overwrite either automation binary while it runs. Ask
  # the installed copy to stop once its runs finish, wait for it to exit, and
  # say whether it was running so an update can start it again. A daemon
  # started from anywhere else is left alone.
  $automate = Join-Path $dest 'sheprd-automate.exe'
  $stopAutomate = {
    $stopped = $false
    foreach ($name in @('sheprd-automate', 'herdr-automate')) {
      $installedAutomate = Join-Path $dest "$name.exe"
      $running = { Get-Process -Name $name -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $installedAutomate } }
      if (-not (& $running)) { continue }
      Write-Host "Stopping automation at $installedAutomate once its runs finish"
      & $installedAutomate shutdown | Out-Null
      if ($LASTEXITCODE -ne 0) { throw "The automation daemon at $installedAutomate did not stop. Finish its runs, then retry." }
      $deadline = (Get-Date).AddSeconds(30)
      while ((& $running) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200 }
      if (& $running) { throw "The automation daemon at $installedAutomate is still running. Finish its runs, then retry." }
      $stopped = $true
    }
    return $stopped
  }

  # "Open in Sheprd" on a folder, and on the background inside one: the
  # launcher runs `sheprd open`, which opens the folder as Open Project
  # does and starts the app first when it is not running. Windows 11 lists it
  # under "Show more options". The Setup.exe installer writes the same keys.
  $verbs = 'HKCU:\Software\Classes\Directory\shell\HerdrUI', 'HKCU:\Software\Classes\Directory\Background\shell\HerdrUI'
  # Keep Explorer's old key and the old link scheme alongside the new scheme
  # so pinned links continue to work. Setup.exe writes these same keys.
  $schemes = @('HKCU:\Software\Classes\sheprd', 'HKCU:\Software\Classes\herdr-gpui')
  $shell = New-Object -ComObject WScript.Shell
  $shortcutFolders = @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('Desktop'))
  $ownedShortcuts = @(
    foreach ($folder in $shortcutFolders) {
      foreach ($name in @('Sheprd.lnk', 'Herdr UI.lnk', 'Herdr.lnk')) {
        $path = Join-Path $folder $name
        if ((Test-Path -LiteralPath $path) -and ($appPaths -contains $shell.CreateShortcut($path).TargetPath)) { $path }
      }
    }
  )

  if ($Uninstall) {
    & $stopAutomate | Out-Null
    & $assertNotRunning
    # Over a Setup.exe installation its own uninstaller removes everything,
    # including its entry in Settings.
    $uninstaller = Join-Path $dest 'unins000.exe'
    if (Test-Path -LiteralPath $uninstaller) {
      $process = Start-Process -Wait -PassThru -FilePath $uninstaller -ArgumentList '/SILENT'
      if ($process.ExitCode -ne 0) { throw "The Sheprd uninstaller exited with code $($process.ExitCode); Sheprd may still be installed in $dest." }
      # The uninstaller hands off to a copy of itself and returns early, so
      # wait for it to remove the app.
      $deadline = (Get-Date).AddMinutes(2)
      while (((Test-Path -LiteralPath $exe) -or (Test-Path -LiteralPath $legacyExe)) -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 1 }
      if ((Test-Path -LiteralPath $exe) -or (Test-Path -LiteralPath $legacyExe)) { throw "The Sheprd uninstaller left app files in $dest." }
    }
    foreach ($verb in $verbs) { Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $verb }
    foreach ($scheme in $schemes) {
      Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $scheme
    }
    foreach ($path in $ownedShortcuts) { Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $path }
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $kept = @($userPath -split ';' | Where-Object { $_ -and $_.TrimEnd([char]92) -ne $dest })
    if ($kept.Count -ne @($userPath -split ';' | Where-Object { $_ }).Count) {
      [Environment]::SetEnvironmentVariable('Path', $kept -join ';', 'User')
    }
    if (Test-Path -LiteralPath $dest) { Remove-Item -Recurse -Force -LiteralPath $dest }
    if (Test-Path -LiteralPath $dest) { throw "Could not remove $dest." }
    Write-Host "Uninstalled Sheprd from $dest. Settings and daemon sessions were kept."
    return
  }

  $repo = 'sheprd-labs/sheprd'
  $os = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture
  $arch = switch ($os) {
    'X64' { 'x86_64' }
    'Arm64' { 'aarch64' }
    default { throw "Sheprd has no Windows build for $os" }
  }

  # The latest release is always the newest stable one; betas are prereleases.
  $tag = (Invoke-RestMethod -UseBasicParsing "https://api.github.com/repos/$repo/releases/latest").tag_name
  if ($tag -notmatch '^v([1-9][0-9]{7}\.[1-9][0-9]*)$') { throw "Unexpected latest release tag: $tag" }
  $version = $Matches[1]

  $name = "Sheprd-$version-$arch-pc-windows-msvc"
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
    if ($expected -notmatch '^[a-fA-F0-9]{64}$') { throw "Invalid checksum for $name.zip" }
    $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash
    if ($actual -ne $expected) { throw "Checksum mismatch for $name.zip" }

    Expand-Archive -Path $zip -DestinationPath $tmp
    foreach ($required in @('sheprd.exe', 'sheprd-launcher.exe', 'herdr-gpui.exe', 'herdr-gpui-launcher.exe', 'sheprd-automate.exe')) {
      if (-not (Test-Path -LiteralPath (Join-Path $tmp "$name\$required") -PathType Leaf)) { throw "Release is missing $required" }
    }
    & $assertNotRunning
    $restartAutomate = & $stopAutomate
    & $assertNotRunning
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    Copy-Item -Recurse -Force -Path (Join-Path $tmp "$name\*") -Destination $dest
  } finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
  }

  # Restart only the new copy when an installed automation daemon was running.
  if ($restartAutomate) {
    & $automate start | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Warning 'Could not start the automation daemon again; Sheprd starts it when it next opens.' }
  }

  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not (($userPath -split ';') -contains $dest)) {
    [Environment]::SetEnvironmentVariable('Path', (@($userPath, $dest) | Where-Object { $_ }) -join ';', 'User')
    Write-Host "Added $dest to your user PATH; open a new terminal to use sheprd."
  }

  # Keep the fallback for portable/manual installs without a launcher.
  $hasLauncher = Test-Path -LiteralPath $launcher
  $target = if ($hasLauncher) { $launcher } else { $exe }
  $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Sheprd.lnk'
  $link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
  $link.TargetPath = $target
  $link.WorkingDirectory = $dest
  $link.Save()
  # Each Sheprd window retains this compatibility AppUserModelID (see
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
    Write-Warning "Could not give the Start menu shortcut Sheprd's taskbar identity: $_"
  }

  # Migrate only shortcuts targeting this installation. Preserve whether the
  # user had a desktop shortcut, and leave unrelated shortcuts alone.
  foreach ($old in $ownedShortcuts) {
    $folder = Split-Path -Parent $old
    $updated = Join-Path $folder 'Sheprd.lnk'
    if ($updated -ne $shortcut) { Copy-Item -LiteralPath $shortcut -Destination $updated -Force }
    if ($old -ne $updated) { Remove-Item -LiteralPath $old -Force }
  }

  # A release without the launcher has no `sheprd open` entry here, so it
  # gets no Explorer entry.
  foreach ($verb in $verbs) {
    if (-not $hasLauncher) {
      Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $verb
      continue
    }
    New-Item -Force -Path "$verb\command" | Out-Null
    Set-Item -LiteralPath $verb -Value 'Open in Sheprd'
    Set-ItemProperty -LiteralPath $verb -Name 'Icon' -Value "`"$exe`",0"
    Set-Item -LiteralPath "$verb\command" -Value "`"$launcher`" open `"%V`""
  }

  # Links open through the launcher as well, so a release without it
  # registers none, and loses one an earlier install left.
  foreach ($scheme in $schemes) {
    if ($hasLauncher) {
      New-Item -Force -Path "$scheme\shell\open\command" | Out-Null
      New-Item -Force -Path "$scheme\DefaultIcon" | Out-Null
      Set-Item -LiteralPath $scheme -Value 'URL:Sheprd link'
      Set-ItemProperty -LiteralPath $scheme -Name 'URL Protocol' -Value ''
      Set-Item -LiteralPath "$scheme\DefaultIcon" -Value "`"$exe`",0"
      Set-Item -LiteralPath "$scheme\shell\open\command" -Value "`"$launcher`" `"%1`""
    } else {
      Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $scheme
    }
  }

  if ($hasLauncher) {
    Write-Host "Installed Sheprd $version to $dest. Start it from the Start menu (Sheprd), by running sheprd-launcher.exe, or from Open in Sheprd on a folder in File Explorer; use sheprd for the command line."
  } else {
    Write-Host "Installed Sheprd $version to $dest. Start it from the Start menu (Sheprd) or with sheprd."
  }
} @args
