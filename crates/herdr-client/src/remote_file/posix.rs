//! The `/bin/sh` scripts. They use only POSIX shell, `wc`, `cat`, `mktemp`,
//! `mv`, and `rm`, plus a SHA-256 tool when a push expects a hash.

use super::{
    CONFLICT_EXIT, Expect, MISSING_EXIT, NO_HASHER_EXIT, NOT_A_FILE_EXIT, SYMLINK_EXIT,
    TOO_LARGE_EXIT, hex,
};
use crate::script::shell_quote;

/// Sets `path` to the host's path, expanding a leading `~/` to `$HOME`.
fn path_assignment(path: &str) -> String {
    if path == "~" {
        return r#"path="$HOME""#.to_owned();
    }
    match path.strip_prefix("~/") {
        Some(rest) => format!(r#"path="$HOME"/{}"#, shell_quote(rest)),
        None => format!("path={}", shell_quote(path)),
    }
}

/// Refuses a missing path, a non-regular file, and a file over `max_bytes`,
/// then streams the file. `-e` and `-f` follow symbolic links.
pub(super) fn fetch_script(path: &str, max_bytes: u64) -> String {
    // The shell's arithmetic is signed; no real limit comes near it.
    let max = max_bytes.min(i64::MAX as u64);
    format!(
        r#"{assign}
if [ ! -e "$path" ]; then exit {MISSING_EXIT}; fi
if [ ! -f "$path" ]; then exit {NOT_A_FILE_EXIT}; fi
size=$(wc -c < "$path") || exit 1
case $size in ''|*[!0-9\ ]*) exit 1 ;; esac
if [ $size -gt {max} ]; then exit {TOO_LARGE_EXIT}; fi
exec cat < "$path""#,
        assign = path_assignment(path),
    )
}

/// Refuses a symbolic link or a non-regular file at the path. A push runs it
/// before and after the copy, so a path that changed kind meanwhile is still
/// refused.
fn refuse_unsafe() -> String {
    format!(
        r#"if [ -L "$path" ]; then exit {SYMLINK_EXIT}; fi
if [ -e "$path" ] && [ ! -f "$path" ]; then exit {NOT_A_FILE_EXIT}; fi"#
    )
}

/// Picks the first SHA-256 tool on the host, before any input is read, so a
/// host without one refuses without receiving the file.
fn choose_hasher() -> String {
    format!(
        r#"if command -v sha256sum >/dev/null 2>&1; then hasher='sha256sum'
elif command -v shasum >/dev/null 2>&1; then hasher='shasum -a 256'
elif command -v openssl >/dev/null 2>&1; then hasher='openssl dgst -sha256 -r'
else exit {NO_HASHER_EXIT}; fi"#
    )
}

/// The check `expect` makes just before the rename. Each hasher reads the file
/// on stdin, so no file name reaches its output or its options.
fn expect_check(expect: Expect) -> String {
    match expect {
        Expect::Sha256(digest) => format!(
            r#"if [ ! -f "$path" ]; then exit {CONFLICT_EXIT}; fi
sum=$($hasher < "$path") || exit 1
if [ "${{sum%% *}}" != '{hex}' ]; then exit {CONFLICT_EXIT}; fi"#,
            hex = hex(&digest),
        ),
        Expect::Absent => {
            format!(r#"if [ -e "$path" ] || [ -L "$path" ]; then exit {CONFLICT_EXIT}; fi"#)
        }
        Expect::Any => String::new(),
    }
}

/// Gives the copy the target's permission bits, read with GNU `stat -c` or
/// BSD `stat -f`, or a new file the mode the caller's umask gives one. When no
/// `stat` reads the target's mode, the copy is written over the target in
/// place instead: that keeps the mode, owner, and hard links, but it is not
/// atomic, so a reader can see a partly written file and a failure partway
/// leaves one.
const KEEP_MODE: &str = r#"if [ -e "$path" ]; then
mode=$(stat -c %a -- "$path" 2>/dev/null) || mode=$(stat -f %Lp -- "$path" 2>/dev/null) || mode=
case $mode in ''|*[!0-7]*) mode= ;; esac
if [ -z "$mode" ]; then
cat < "$tmp" > "$path" || exit 1
exit 0
fi
chmod -- "$mode" "$tmp" || exit 1
else
chmod -- "$(printf '%o' $(( 0666 & ~0$old_umask )))" "$tmp" || exit 1
fi"#;

/// Copies stdin to a private temporary file beside the target, checks
/// `expect`, gives the copy the target's permission bits (see [`KEEP_MODE`]),
/// and renames it over the target, so the file becomes the SSH user's. The
/// trap removes the copy on every exit that did not rename it.
pub(super) fn push_script(path: &str, expect: Expect) -> String {
    let hasher = match expect {
        Expect::Sha256(_) => choose_hasher(),
        Expect::Absent | Expect::Any => String::new(),
    };
    format!(
        r#"old_umask=$(umask)
umask 077
{assign}
{refuse}
{hasher}
case $path in
*/*) dir=${{path%/*}} ;;
*) dir=. ;;
esac
if [ -z "$dir" ]; then dir=/; fi
name=${{path##*/}}
tmp=$(mktemp "$dir/.$name.XXXXXX") || exit 1
trap 'rm -f -- "$tmp"' EXIT
cat > "$tmp" || exit 1
{refuse}
{check}
{KEEP_MODE}
mv -f -- "$tmp" "$path" || exit 1
trap - EXIT"#,
        assign = path_assignment(path),
        refuse = refuse_unsafe(),
        check = expect_check(expect),
    )
}
