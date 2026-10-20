//! Windows OpenSSH hosts, adapted from upstream remote/attach.rs. Their
//! default shell is `cmd.exe` or PowerShell, so the POSIX bridge script never
//! runs there; every command goes through `powershell.exe -EncodedCommand`,
//! which both shells start the same way and which needs no quoting beyond
//! base64. Discovery and the bridge are separate connections: PowerShell
//! buffers stdin it reads, so it cannot answer a handshake and then hand the
//! rest of the stream to `herdr.exe` as the POSIX script does.
use super::{PROBE_CANDIDATE, PROBE_DONE, READY};
use base64::{Engine as _, engine::general_purpose::STANDARD};

/// Precedes each candidate's base64 UTF-8 path, inside its probe block.
const PATH_MARKER: &str = "herdr-windows-path:";

/// Whether a remote shell exit before the bridge was ready means the host
/// could not run `/bin/sh`. `ssh` exits 255 for its own failures and the
/// bridge script 127 when it found no Herdr; anything else is the shell.
pub(super) fn rejected_posix(code: Option<i32>) -> bool {
    code.is_some_and(|code| !matches!(code, 0 | 127 | 255))
}

/// The probe for a Windows host: the same candidate blocks as
/// [`super::probe_script`], each also naming its executable's path.
pub(super) fn probe_command(session: &str) -> String {
    encoded_command(&format!(
        r#"$herdrPaths = [System.Collections.Generic.List[string]]::new(); function Add-HerdrPath([string]$CandidatePath) {{ if ([string]::IsNullOrWhiteSpace($CandidatePath) -or -not (Test-Path -LiteralPath $CandidatePath -PathType Leaf)) {{ return }}; $fullPath = [System.IO.Path]::GetFullPath($CandidatePath); if ($herdrPaths -notcontains $fullPath) {{ $herdrPaths.Add($fullPath) }} }}; $pathCommand = Get-Command herdr.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1; if ($null -ne $pathCommand) {{ Add-HerdrPath $pathCommand.Source }}; $userSid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value; Get-CimInstance Win32_Process -Filter "Name = 'herdr.exe' OR Name = 'herdr-dev.exe'" -ErrorAction SilentlyContinue | ForEach-Object {{ $owner = Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid -ErrorAction SilentlyContinue; if ($owner.ReturnValue -eq 0 -and $owner.Sid -eq $userSid) {{ Add-HerdrPath $_.ExecutablePath }} }}; $herdrHome = if ([string]::IsNullOrWhiteSpace($env:HERDR_HOME)) {{ Join-Path $env:USERPROFILE '.herdr' }} else {{ $env:HERDR_HOME }}; $activeJunction = Get-Item -LiteralPath (Join-Path $herdrHome 'packages\standalone\current') -Force -ErrorAction SilentlyContinue; if ($null -ne $activeJunction -and -not [string]::IsNullOrWhiteSpace([string]$activeJunction.Target)) {{ Add-HerdrPath (Join-Path ([string]$activeJunction.Target) 'herdr.exe') }}; foreach ($herdrPath in $herdrPaths) {{ try {{ $status = (& $herdrPath status client --json 2>$null) -join ''; if ($LASTEXITCODE -ne 0) {{ continue }} }} catch {{ continue }}; try {{ $server = (& $herdrPath --session {session} status server --json 2>$null) -join ''; if ($LASTEXITCODE -ne 0) {{ $server = '' }} }} catch {{ $server = '' }}; [Console]::Out.WriteLine('{PROBE_CANDIDATE}'); [Console]::Out.WriteLine('{PATH_MARKER}' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($herdrPath))); [Console]::Out.WriteLine($status); [Console]::Out.WriteLine($server) }}; [Console]::Out.WriteLine('{PROBE_DONE}'); exit 0"#,
        session = quote_powershell_arg(session),
    ))
}

/// A probe block's executable path and `status client --json` object.
pub(super) struct Candidate {
    pub(super) path: String,
    pub(super) status: serde_json::Value,
}

/// Every complete candidate block the probe printed, in its order. A block
/// without a valid path or client status is dropped: it cannot be bridged.
pub(super) fn candidates(output: &[u8]) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    let mut path = None;
    let mut status = None;
    for line in super::lines(output) {
        if line == PROBE_CANDIDATE.as_bytes() || line == PROBE_DONE.as_bytes() {
            if let (Some(path), Some(status)) = (path.take(), status.take()) {
                candidates.push(Candidate { path, status });
            }
            path = None;
            status = None;
            if line == PROBE_DONE.as_bytes() {
                break;
            }
        } else if let Some(encoded) = line.strip_prefix(PATH_MARKER.as_bytes()) {
            path = decode_path(encoded);
        } else if status.is_none() {
            status = serde_json::from_slice(line).ok();
        }
    }
    candidates
}

fn decode_path(encoded: &[u8]) -> Option<String> {
    let path = String::from_utf8(STANDARD.decode(encoded).ok()?).ok()?;
    (!path.is_empty() && !path.chars().any(char::is_control)).then_some(path)
}

/// [`super::compatible`], plus the bridge upstream requires of a Windows host.
pub(super) fn compatible(status: &serde_json::Value) -> Option<bool> {
    if status["remote_host_bridge"].as_bool() != Some(true) {
        return None;
    }
    super::compatible(status)
}

/// Run `path`'s bridge for `session` after printing the ready banner.
/// `Start-Process -Wait` semantics keep PowerShell alive for a cold-started
/// server, and holding the handle keeps the exit code on PowerShell 5.1.
pub(super) fn bridge_command(path: &str, session: &str, idle_timeout: bool) -> String {
    let mut args = vec!["--session", session, "remote-client-bridge"];
    if idle_timeout {
        args.push("--idle-timeout-v1");
    }
    let command_line = args
        .iter()
        .map(|arg| quote_windows_command_line_arg(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let ready = String::from_utf8_lossy(READY.trim_ascii_end());
    encoded_command(&format!(
        "[Console]::Out.WriteLine(); [Console]::Out.WriteLine('{ready}'); [Console]::Out.Flush(); $process = Start-Process -FilePath {} -ArgumentList {} -NoNewWindow -PassThru -ErrorAction Stop; $null = $process.Handle; $process.WaitForExit(); exit $process.ExitCode",
        quote_powershell_arg(path),
        quote_powershell_arg(&command_line),
    ))
}

/// `script` as a `powershell.exe` command line, encoded as UTF-16LE base64
/// so neither `cmd.exe` nor PowerShell as the login shell reinterprets it.
pub(super) fn encoded_command(script: &str) -> String {
    let utf16 = script
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    format!(
        "powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand {}",
        STANDARD.encode(utf16)
    )
}

fn quote_powershell_arg(value: &str) -> String {
    if !value.is_empty()
        && !value.starts_with('-')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'+' | b'=')
        })
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "''"))
}

/// One argument as `CommandLineToArgvW` splits it back out.
fn quote_windows_command_line_arg(value: &str) -> String {
    if !value.is_empty()
        && !value
            .chars()
            .any(|ch| matches!(ch, ' ' | '\t' | '\n' | '\x0b' | '"'))
    {
        return value.to_owned();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            backslashes += 1;
            continue;
        }
        let escaped = if ch == '"' {
            backslashes * 2 + 1
        } else {
            backslashes
        };
        quoted.extend(std::iter::repeat_n('\\', escaped));
        backslashes = 0;
        quoted.push(ch);
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}
