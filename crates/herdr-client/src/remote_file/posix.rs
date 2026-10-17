//! The `/bin/sh` scripts. They use only POSIX shell, `wc`, `cat`, `mktemp`,
//! `mv`, and `rm`, plus a SHA-256 tool when a push expects a hash, and `stat`,
//! `ls`, `getfacl` where there is one, `chgrp`, and `chmod` to keep a file's
//! access.

use super::{
    ACCESS_EXIT, Access, CONFLICT_EXIT, Expect, MISSING_EXIT, NO_HASHER_EXIT, NOT_A_FILE_EXIT,
    SHORT_EXIT, SYMLINK_EXIT, TOO_LARGE_EXIT, hex,
};
use crate::script::shell_quote;

/// Sets `path` to the host's path, expanding a leading `~/` to `$HOME`.
fn path_assignment(path: &str) -> String {
    assignment("path", path)
}

/// Sets the shell variable `name` to the host's `path`, expanding a leading
/// `~/` to `$HOME`.
fn assignment(name: &str, path: &str) -> String {
    if path == "~" {
        return format!(r#"{name}="$HOME""#);
    }
    match path.strip_prefix("~/") {
        Some(rest) => format!(r#"{name}="$HOME"/{}"#, shell_quote(rest)),
        None => format!("{name}={}", shell_quote(path)),
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

/// Gives the copy the group and permission bits of `$like`, the target when
/// it exists and otherwise `$source` (empty for [`Access::Keep`]), read with
/// GNU `stat -c` or BSD `stat -f`. `chgrp` runs only when the groups differ,
/// and before `chmod`, which restores any bit `chgrp` clears. When `chgrp`
/// fails, the copy keeps its own group, which is enough only when the group
/// and other bits are identical: otherwise members of the old group who are
/// not in the copy's would fall into "other" and gain or lose access.
///
/// The access is not kept when it cannot be read or copied: `$like` is a
/// symbolic link or not a regular file (GNU `stat` reads a link as 777, and a
/// folder has a mode of its own), no `stat` reads the mode or group, `ls -ld`
/// fails, `ls -ld` marks `$like` or the copy with `+` or macOS's `@` (an ACL,
/// or extended attributes that may hide one), or `getfacl -s` lists an ACL
/// on either (BusyBox `ls` never marks one). A folder's default ACL reaches
/// the copy, and `chmod` would raise its mask.
///
/// Then an existing target is written over in place, which keeps its mode,
/// owner, group, ACL, and hard links but is not atomic, so a reader can see a
/// partly written file and a failure partway leaves one; a target the SSH
/// user cannot write is refused with [`ACCESS_EXIT`] before the host's file
/// changes. A new file whose source's access is not kept is never refused:
/// it stays readable by the SSH user alone. One with no source gets the mode
/// the umask gives it.
fn keep_access() -> String {
    format!(
        r#"if [ -e "$path" ]; then like=$path; else like=$source; fi
keep=
if [ -n "$like" ]; then
mode=$(stat -c %a -- "$like" 2>/dev/null) || mode=$(stat -f %Lp -- "$like" 2>/dev/null) || mode=
case $mode in ''|*[!0-7]*) mode= ;; esac
group=$(stat -c %g -- "$like" 2>/dev/null) || group=$(stat -f %g -- "$like" 2>/dev/null) || group=
case $group in ''|*[!0-9]*) group= ;; esac
tmp_group=$(stat -c %g -- "$tmp" 2>/dev/null) || tmp_group=$(stat -f %g -- "$tmp" 2>/dev/null) || tmp_group=
if [ -n "$mode" ] && [ -n "$group" ]; then keep=1; fi
if [ -L "$like" ] || [ ! -f "$like" ]; then keep=; fi
for listed in "$like" "$tmp"; do
listing=$(ls -ld -- "$listed" 2>/dev/null) || listing=
case $listing in ''|??????????[+@]*) keep= ;; esac
if command -v getfacl >/dev/null 2>&1 && [ -n "$(getfacl -s -- "$listed" 2>/dev/null)" ]; then keep=; fi
done
if [ -n "$keep" ] && [ "$group" != "$tmp_group" ] && ! chgrp -- "$group" "$tmp" 2>/dev/null; then
g=$(( (0$mode >> 3) & 7 ))
o=$(( 0$mode & 7 ))
if [ "$g" -ne "$o" ]; then keep=; fi
fi
fi
if [ -n "$keep" ]; then
chmod -- "$mode" "$tmp" || exit 1
elif [ -e "$path" ]; then
if [ ! -w "$path" ]; then exit {ACCESS_EXIT}; fi
cat < "$tmp" > "$path" || exit 1
exit 0
elif [ -z "$like" ]; then
chmod -- "$(printf '%o' $(( 0666 & ~0$old_umask )))" "$tmp" || exit 1
fi"#
    )
}

/// Copies stdin to a private temporary file beside the target, refuses it
/// unless it holds exactly `len` bytes, checks `expect`, gives the copy the
/// access of the target or of `access`'s source, and renames it over the
/// target, so the file becomes the SSH user's. When that access cannot be
/// kept, it writes over an existing target in place instead (see
/// [`keep_access`]). The trap removes the copy on every exit that did not
/// rename it.
pub(super) fn push_script(path: &str, expect: Expect, access: Access<'_>, len: u64) -> String {
    // The shell's arithmetic is signed; no real length comes near it.
    let len = len.min(i64::MAX as u64);
    let hasher = match expect {
        Expect::Sha256(_) => choose_hasher(),
        Expect::Absent | Expect::Any => String::new(),
    };
    let source = match access {
        Access::Keep => "source=".to_owned(),
        Access::Like(source) => assignment("source", source),
    };
    format!(
        r#"old_umask=$(umask)
umask 077
{assign}
{source}
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
size=$(wc -c < "$tmp") || exit 1
case $size in ''|*[!0-9\ ]*) exit 1 ;; esac
if [ $size -ne {len} ]; then exit {SHORT_EXIT}; fi
{refuse}
{check}
{keep}
mv -f -- "$tmp" "$path" || exit 1
trap - EXIT"#,
        assign = path_assignment(path),
        refuse = refuse_unsafe(),
        check = expect_check(expect),
        keep = keep_access(),
    )
}
