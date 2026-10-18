# Installs the latest stable Herdr UI release for the current user on Windows,
# served at https://herdrui.dev/install.ps1:
#
#   irm https://herdrui.dev/install.ps1 | iex
#
# It downloads the zip for this machine from GitHub Releases, checks it against
# the release's published SHA-256, installs herdr-gpui.exe and
# herdr-gpui-launcher.exe under %LOCALAPPDATA%\Programs\Herdr UI, adds that
# directory to the user PATH so herdr-gpui works as a command, and creates a
# Start menu shortcut to herdr-gpui-launcher.exe, the windowed launcher that
# starts herdr-gpui.exe without a console window. It never installs, starts or
# upgrades the Herdr daemon. Windows builds are experimental and never update
# themselves: run this again to update. It also adds "Open in Herdr UI" to
# folders in File Explorer, for the current user only.
#
# To uninstall, run it with -Uninstall:
#
#   & ([scriptblock]::Create((irm https://herdrui.dev/install.ps1))) -Uninstall
#
# That removes what this script added: the files, the PATH entry, the Start
# menu shortcut, and the Explorer entry. Settings and Herdr sessions are kept.
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

  # "Open in Herdr UI" on a folder, and on the background inside one: the
  # launcher runs `herdr-gpui open`, which opens the folder as Open Project
  # does and starts the app first when it is not running. Windows 11 lists it
  # under "Show more options". The Setup.exe installer writes the same keys.
  $verbs = 'HKCU:\Software\Classes\Directory\shell\HerdrUI', 'HKCU:\Software\Classes\Directory\Background\shell\HerdrUI'

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
    Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath (Join-Path ([Environment]::GetFolderPath('Programs')) 'Herdr UI.lnk')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $kept = @($userPath -split ';' | Where-Object { $_ -and $_.TrimEnd([char]92) -ne $dest })
    if ($kept.Count -ne @($userPath -split ';' | Where-Object { $_ }).Count) {
      [Environment]::SetEnvironmentVariable('Path', $kept -join ';', 'User')
    }
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
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    Copy-Item -Recurse -Force -Path (Join-Path $tmp "$name\*") -Destination $dest
  } finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
  }

  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not (($userPath -split ';') -contains $dest)) {
    [Environment]::SetEnvironmentVariable('Path', (@($userPath, $dest) | Where-Object { $_ }) -join ';', 'User')
    Write-Host "Added $dest to your user PATH; open a new terminal to use herdr-gpui."
  }

  $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Herdr UI.lnk'
  $link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
  $link.TargetPath = $launcher
  $link.WorkingDirectory = $dest
  $link.Save()

  foreach ($verb in $verbs) {
    New-Item -Force -Path "$verb\command" | Out-Null
    Set-Item -LiteralPath $verb -Value 'Open in Herdr UI'
    Set-ItemProperty -LiteralPath $verb -Name 'Icon' -Value "`"$exe`",0"
    Set-Item -LiteralPath "$verb\command" -Value "`"$launcher`" open `"%V`""
  }

  Write-Host "Installed Herdr UI $version to $dest. Start it from the Start menu (Herdr UI), by running herdr-gpui-launcher.exe, or from Open in Herdr UI on a folder in File Explorer; use herdr-gpui for the command line."
  if (-not (Get-Command herdr -ErrorAction SilentlyContinue)) {
    Write-Host 'Herdr UI needs the Herdr daemon, which is not on PATH. Install it from https://herdr.dev/'
  }
} @args
