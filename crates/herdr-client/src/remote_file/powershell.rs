//! The PowerShell scripts, for Windows OpenSSH hosts. They need only Windows
//! PowerShell 5.1 and .NET Framework, and move bytes through the raw console
//! streams: PowerShell's own pipeline would re-encode them as text.

use super::{
    CONFLICT_EXIT, Expect, MISSING_EXIT, NOT_A_FILE_EXIT, SYMLINK_EXIT, TOO_LARGE_EXIT, hex,
};

/// `FileAttributes.ReparsePoint`: symbolic links, junctions, and other
/// redirections, none of which a push replaces.
const REPARSE_POINT: u32 = 0x400;
/// `FileAttributes.Directory | FileAttributes.Device`.
const NOT_A_FILE: u32 = 0x10 | 0x40;

const PREAMBLE: &str = "$ErrorActionPreference = 'Stop'\n$ProgressPreference = 'SilentlyContinue'";

/// `value` as a single-quoted PowerShell string. PowerShell also ends a
/// single-quoted string at the typographic single quotes, so each of those is
/// doubled too.
fn literal(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        if matches!(ch, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            quoted.push(ch);
        }
        quoted.push(ch);
    }
    quoted.push('\'');
    quoted
}

/// The host's path as an expression, expanding a leading `~\` or `~/` to the
/// user's profile folder.
fn path_expression(path: &str) -> String {
    if path == "~" {
        return "$env:USERPROFILE".to_owned();
    }
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => format!(
            "[System.IO.Path]::Combine($env:USERPROFILE, {})",
            literal(rest)
        ),
        None => literal(path),
    }
}

/// Refuses a missing path, a folder, and a file over `max_bytes`, then copies
/// the file to stdout. Opening the file follows symbolic links.
pub(super) fn fetch_script(path: &str, max_bytes: u64) -> String {
    let max = max_bytes.min(i64::MAX as u64);
    format!(
        r#"{PREAMBLE}
$path = {path}
if ([System.IO.Directory]::Exists($path)) {{ exit {NOT_A_FILE_EXIT} }}
if (-not [System.IO.File]::Exists($path)) {{ exit {MISSING_EXIT} }}
try {{
    $file = [System.IO.File]::Open($path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
}} catch [System.IO.FileNotFoundException], [System.IO.DirectoryNotFoundException] {{
    exit {MISSING_EXIT}
}}
try {{
    if ($file.Length -gt {max}) {{ exit {TOO_LARGE_EXIT} }}
    $stdout = [Console]::OpenStandardOutput()
    $file.CopyTo($stdout)
    $stdout.Flush()
}} finally {{
    $file.Dispose()
}}
exit 0"#,
        path = path_expression(path),
    )
}

/// The check `expect` makes just before the rename. The hash comes from .NET
/// rather than `Get-FileHash`, whose module fails to load in Windows
/// PowerShell when `PSModulePath` names PowerShell 7's modules first.
fn expect_check(expect: Expect) -> String {
    match expect {
        Expect::Sha256(digest) => format!(
            r#"if (-not [System.IO.File]::Exists($path)) {{ exit {CONFLICT_EXIT} }}
    $current = [System.IO.File]::Open($path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {{
        $digest = [System.Security.Cryptography.SHA256]::Create().ComputeHash($current)
    }} finally {{
        $current.Dispose()
    }}
    $actual = [System.BitConverter]::ToString($digest).Replace('-', '').ToLowerInvariant()
    if ($actual -ne '{hex}') {{ exit {CONFLICT_EXIT} }}"#,
            hex = hex(&digest),
        ),
        Expect::Absent => {
            format!(r#"if ($null -ne (Get-HerdrAttributes $path)) {{ exit {CONFLICT_EXIT} }}"#)
        }
        Expect::Any => String::new(),
    }
}

/// Copies stdin to a hidden-named temporary file beside the target, checks
/// `expect`, and replaces the target with it. `finally` removes the copy on
/// every exit that did not rename it. `[NullString]::Value` asks `Replace` for
/// no backup file; PowerShell would pass `$null` as an empty string.
pub(super) fn push_script(path: &str, expect: Expect) -> String {
    format!(
        r#"{PREAMBLE}
$path = [System.IO.Path]::GetFullPath({path})
function Get-HerdrAttributes([string]$Target) {{
    try {{
        return [System.IO.File]::GetAttributes($Target)
    }} catch [System.IO.FileNotFoundException], [System.IO.DirectoryNotFoundException] {{
        return $null
    }}
}}
function Assert-HerdrReplaceable {{
    $attributes = Get-HerdrAttributes $path
    if ($null -eq $attributes) {{ return }}
    $flags = [int]$attributes
    if (($flags -band {REPARSE_POINT}) -ne 0) {{ exit {SYMLINK_EXIT} }}
    if (($flags -band {NOT_A_FILE}) -ne 0) {{ exit {NOT_A_FILE_EXIT} }}
}}
Assert-HerdrReplaceable
$folder = [System.IO.Path]::GetDirectoryName($path)
$name = '.' + [System.IO.Path]::GetFileName($path) + '.' + [System.IO.Path]::GetRandomFileName()
$temporary = [System.IO.Path]::Combine($folder, $name)
$copy = [System.IO.File]::Open($temporary, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
$renamed = $false
try {{
    try {{
        $stdin = [Console]::OpenStandardInput()
        $stdin.CopyTo($copy)
    }} finally {{
        $copy.Dispose()
    }}
    Assert-HerdrReplaceable
    {check}
    if ([System.IO.File]::Exists($path)) {{
        [System.IO.File]::Replace($temporary, $path, [NullString]::Value)
    }} else {{
        [System.IO.File]::Move($temporary, $path)
    }}
    $renamed = $true
}} finally {{
    if (-not $renamed) {{ [System.IO.File]::Delete($temporary) }}
}}
exit 0"#,
        path = path_expression(path),
        check = expect_check(expect),
    )
}
