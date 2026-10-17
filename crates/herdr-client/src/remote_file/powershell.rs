//! The PowerShell scripts, for Windows OpenSSH hosts. They need only Windows
//! PowerShell 5.1 and .NET Framework, and move bytes through the raw console
//! streams: PowerShell's own pipeline would re-encode them as text.

use super::{
    ACCESS_EXIT, Access, CONFLICT_EXIT, Expect, MISSING_EXIT, NOT_A_FILE_EXIT, SHORT_EXIT,
    SYMLINK_EXIT, TOO_LARGE_EXIT, hex,
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

/// Copies stdin to a dot-named temporary file beside the target, refuses it
/// unless it holds exactly `len` bytes, checks `expect`, and replaces the
/// target with it. `finally` removes the copy on every exit that did not
/// rename it.
///
/// The copy is created with the DACL of the target when it exists, else of
/// `access`'s source, so nobody who cannot read that file can read the upload
/// while it is in the copy; `Replace` then keeps the target's DACL. A new
/// file with [`Access::Keep`] takes the folder's inherited ACL. Windows
/// PowerShell (and PowerShell 3 to 5.0, which have no `PSEdition`) creates
/// the copy through the `FileStream` constructor taking a `FileSecurity`,
/// and PowerShell 7 through `FileSystemAclExtensions.Create`. When the source
/// is a reparse point or a folder, whose ACL is no file's to copy, or its
/// DACL cannot be read, the push exits with [`ACCESS_EXIT`] before it creates
/// the copy. `[NullString]::Value` asks `Replace` for no backup file; PowerShell
/// would pass `$null` as an empty string.
pub(super) fn push_script(path: &str, expect: Expect, access: Access<'_>, len: u64) -> String {
    let source = match access {
        Access::Keep => "$null".to_owned(),
        Access::Like(source) => {
            format!("[System.IO.Path]::GetFullPath({})", path_expression(source))
        }
    };
    format!(
        r#"{PREAMBLE}
$path = [System.IO.Path]::GetFullPath({path})
$source = {source}
$core = $PSVersionTable.PSEdition -eq 'Core'
$sections = [System.Security.AccessControl.AccessControlSections]::Access
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
$like = if ([System.IO.File]::Exists($path)) {{ $path }} else {{ $source }}
$security = $null
if ($null -ne $like) {{
    try {{
        $likeAttributes = Get-HerdrAttributes $like
        if ($null -ne $likeAttributes -and ([int]$likeAttributes -band ({REPARSE_POINT} -bor {NOT_A_FILE})) -ne 0) {{ exit {ACCESS_EXIT} }}
        $security = if ($core) {{ [System.IO.FileSystemAclExtensions]::GetAccessControl([System.IO.FileInfo]$like, $sections) }} else {{ [System.IO.File]::GetAccessControl($like, $sections) }}
    }} catch {{
        exit {ACCESS_EXIT}
    }}
}}
$write = [System.Security.AccessControl.FileSystemRights]::Write
$copy = if ($null -eq $security) {{
    [System.IO.File]::Open($temporary, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
}} elseif ($core) {{
    [System.IO.FileSystemAclExtensions]::Create([System.IO.FileInfo]$temporary, [System.IO.FileMode]::CreateNew, $write, [System.IO.FileShare]::None, 4096, [System.IO.FileOptions]::None, $security)
}} else {{
    New-Object System.IO.FileStream($temporary, [System.IO.FileMode]::CreateNew, $write, [System.IO.FileShare]::None, 4096, [System.IO.FileOptions]::None, $security)
}}
$renamed = $false
$received = -1
try {{
    try {{
        $stdin = [Console]::OpenStandardInput()
        $stdin.CopyTo($copy)
        $received = $copy.Length
    }} finally {{
        $copy.Dispose()
    }}
    if ($received -ne {len}) {{ exit {SHORT_EXIT} }}
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
