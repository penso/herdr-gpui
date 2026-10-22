//! The `/bin/sh` scripts. They use only POSIX shell, `wc`, `cat`, `mktemp`,
//! `mv`, `rm`, `rmdir`, `id`, and `ls`, plus a SHA-256 tool when a push
//! expects a hash, and `stat`, `getfacl` where there is one, `chgrp`, and
//! `chmod` to keep a file's access.

use super::{
    ACCESS_EXIT, Access, CONFLICT_EXIT, Expect, MISSING_EXIT, NO_HASHER_EXIT, NOT_A_FILE_EXIT,
    SHARED_FOLDER_EXIT, SHORT_EXIT, SYMLINK_EXIT, TOO_LARGE_EXIT, hex,
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

/// Refuses a symbolic link or a non-regular file at the path the shell
/// variable `var` holds. A push runs it before and after the copy, so a path
/// that changed kind meanwhile is still refused.
fn refuse_unsafe(var: &str) -> String {
    format!(
        r#"if [ -L "${var}" ]; then exit {SYMLINK_EXIT}; fi
if [ -e "${var}" ] && [ ! -f "${var}" ]; then exit {NOT_A_FILE_EXIT}; fi"#
    )
}

/// `inode PATH` prints the inode number `ls -di` reports for `PATH`, or
/// nothing when `ls` fails.
const INODE_FUNCTION: &str = r#"inode() {
set -f
set -- $(ls -di -- "$1" 2>/dev/null)
set +f
printf '%s' "${1-}"
}"#;

/// `folder_is_safe` succeeds when no one but the SSH user and root can
/// replace `./$name` in the working directory, the anchored target folder,
/// read with POSIX `ls -ldn`. Run after `cd -P`, so it describes the folder
/// rather than a link to it.
const FOLDER_IS_SAFE_FUNCTION: &str = r#"folder_is_safe() {
nl='
'
folder_listing=$(ls -ldn . 2>/dev/null) || return 1
set -f
set -- $folder_listing
set +f
folder_mode=${1-}
# The folder is the SSH user's or root's.
case ${3-} in "$uid"|0) ;; *) return 1 ;; esac
# No ACL shows on it: `+`, or macOS's `@` (extended attributes, which hide
# `+`) when `ls -lde` lists an ACL entry after the first line.
case $folder_mode in
??????????+*) return 1 ;;
??????????@*)
folder_acl=$(ls -lde . 2>/dev/null) || return 1
case $folder_acl in *"$nl"*) return 1 ;; esac ;;
esac
# `getfacl -s`, where there is one, lists no ACL on it (BusyBox `ls` marks none).
if command -v getfacl >/dev/null 2>&1 && [ -n "$(getfacl -s . 2>/dev/null)" ]; then return 1; fi
# Neither its group nor others may write it...
case $folder_mode in ?????w*|????????w*) ;; *) return 0 ;; esac
# ...or it is sticky and the target is the SSH user's or root's, so no one
# else may rename or remove it.
case $folder_mode in ?????????[tT]*) ;; *) return 1 ;; esac
set -f
set -- $(ls -ldn -- "./$name" 2>/dev/null)
set +f
case ${3-} in "$uid"|0) return 0 ;; esac
return 1
}"#;

/// The `EXIT` trap: removes the names the push made with `mktemp` inside the
/// work folder and still holds (the copy and the probe files), then the
/// emptied folder from the anchored folder, never following a name another
/// user could have replaced (`rmdir` removes no link and no folder with
/// anything in it). It removes nothing else, so a folder of the SSH user's
/// own swapped in for the work folder keeps every file it held.
const CLEANUP_FUNCTION: &str = r#"cleanup() {
if [ -n "$inside" ]; then
for made in "$copy" "$probe" "$probe_to"; do
if [ -n "$made" ]; then rm -f -- "$made"; fi
done
cd .. || return
if [ "$(inode .)" != "$here" ]; then return; fi
fi
if [ -n "$work" ]; then rmdir -- "$work" 2>/dev/null; fi
}"#;

/// Enters the private work folder and refuses unless it is the SSH user's
/// `drwx------` folder inside the anchored folder.
fn enter_work_folder() -> String {
    format!(
        r#"cd -P -- "$work" || exit 1
inside=1
work_listing=$(ls -ldn . 2>/dev/null) || work_listing=
set -f
set -- $work_listing
set +f
case ${{1-}} in drwx------*) ;; *) exit {SHARED_FOLDER_EXIT} ;; esac
if [ "${{3-}}" != "$uid" ] || [ "$(inode ..)" != "$here" ]; then exit {SHARED_FOLDER_EXIT}; fi"#
    )
}

/// Sets `no_follow` to the `mv` option that renames over a link to a folder
/// rather than into the folder: GNU's `-T` or BSD's `-h`, tried by renaming
/// one file the push just made in the work folder onto another, or nothing
/// when `mv` takes neither. Only those two new names are ever replaced or
/// removed.
const PICK_NO_FOLLOW: &str = r#"probe=$(mktemp ./p.XXXXXX) || exit 1
probe_to=$(mktemp ./q.XXXXXX) || exit 1
if mv -T -- "$probe" "$probe_to" 2>/dev/null; then no_follow=-T; probe=
elif mv -h -- "$probe" "$probe_to" 2>/dev/null; then no_follow=-h; probe=
else no_follow=
fi
for made in "$probe" "$probe_to"; do
if [ -n "$made" ]; then rm -f -- "$made"; fi
done
probe=
probe_to="#;

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
            r#"if [ ! -f "$target" ]; then exit {CONFLICT_EXIT}; fi
sum=$($hasher < "$target") || exit 1
if [ "${{sum%% *}}" != '{hex}' ]; then exit {CONFLICT_EXIT}; fi"#,
            hex = hex(&digest),
        ),
        Expect::Absent => absent_check(),
        Expect::Any => String::new(),
    }
}

/// Refuses a path where something now exists, even a dangling link.
fn absent_check() -> String {
    format!(r#"if [ -e "$target" ] || [ -L "$target" ]; then exit {CONFLICT_EXIT}; fi"#)
}

/// Gives the copy the group and permission bits of `$like`, the target when
/// it exists and otherwise `$source` (empty for [`Access::Keep`]), read with
/// GNU `stat -c` or BSD `stat -f`. `chgrp` runs only when the groups differ,
/// and before `chmod`, which restores any bit `chgrp` clears. When `chgrp`
/// fails, the copy keeps its own group, which is enough only when the group
/// and other bits are identical: otherwise members of the old group who are
/// not in the copy's would fall into "other" and gain or lose access.
///
/// A set-user-ID bit is kept only when the copy's owner is `$like`'s owner:
/// when either owner cannot be read, or they differ, the bit is dropped, so
/// the file never runs as an account it did not before. A set-group-ID bit
/// is dropped the same way when `chgrp` fails and the copy keeps its own
/// group. Sticky stays: on a regular file it names no owner or group.
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
/// partly written file and a failure partway leaves one. That write opens the
/// target by name, so it happens only where no one else can replace that
/// name: a push expecting nothing there refuses with [`CONFLICT_EXIT`], one
/// in a folder that is not safe (see [`FOLDER_IS_SAFE_FUNCTION`]) with
/// [`SHARED_FOLDER_EXIT`], and one over a target the SSH user cannot write
/// with [`ACCESS_EXIT`], each before the host's file changes. A new file
/// whose source's access is not kept is never refused: it stays readable by
/// the SSH user alone. One with no source gets the mode the umask gives it.
fn keep_access(expect: Expect) -> String {
    let absent = match expect {
        Expect::Absent => absent_check(),
        Expect::Sha256(_) | Expect::Any => String::new(),
    };
    format!(
        r#"if [ -e "$target" ]; then like=$target; else like=$source; fi
keep=
if [ -n "$like" ]; then
mode=$(stat -c %a -- "$like" 2>/dev/null) || mode=$(stat -f %Lp -- "$like" 2>/dev/null) || mode=
case $mode in ''|*[!0-7]*) mode= ;; esac
group=$(stat -c %g -- "$like" 2>/dev/null) || group=$(stat -f %g -- "$like" 2>/dev/null) || group=
case $group in ''|*[!0-9]*) group= ;; esac
tmp_group=$(stat -c %g -- "$copy" 2>/dev/null) || tmp_group=$(stat -f %g -- "$copy" 2>/dev/null) || tmp_group=
owner=$(stat -c %u -- "$like" 2>/dev/null) || owner=$(stat -f %u -- "$like" 2>/dev/null) || owner=
case $owner in ''|*[!0-9]*) owner= ;; esac
tmp_owner=$(stat -c %u -- "$copy" 2>/dev/null) || tmp_owner=$(stat -f %u -- "$copy" 2>/dev/null) || tmp_owner=
if [ -n "$mode" ] && [ -n "$group" ]; then keep=1; fi
if [ -L "$like" ] || [ ! -f "$like" ]; then keep=; fi
for listed in "$like" "$copy"; do
listing=$(ls -ld -- "$listed" 2>/dev/null) || listing=
case $listing in ''|??????????[+@]*) keep= ;; esac
if command -v getfacl >/dev/null 2>&1 && [ -n "$(getfacl -s -- "$listed" 2>/dev/null)" ]; then keep=; fi
done
if [ -n "$keep" ] && [ "$group" != "$tmp_group" ] && ! chgrp -- "$group" "$copy" 2>/dev/null; then
g=$(( (0$mode >> 3) & 7 ))
o=$(( 0$mode & 7 ))
if [ "$g" -ne "$o" ]; then
keep=
else
mode=$(printf '%o' $(( 0$mode & ~02000 )))
fi
fi
fi
if [ -n "$keep" ]; then
if [ -z "$owner" ] || [ -z "$tmp_owner" ] || [ "$owner" != "$tmp_owner" ]; then
mode=$(printf '%o' $(( 0$mode & ~04000 )))
fi
chmod -- "$mode" "$copy" || exit 1
elif [ -e "$target" ]; then
{absent}
if [ -z "$safe" ]; then exit {SHARED_FOLDER_EXIT}; fi
if [ ! -w "$target" ]; then exit {ACCESS_EXIT}; fi
cat < "$copy" > "$target" || exit 1
exit 0
elif [ -z "$like" ]; then
chmod -- "$(printf '%o' $(( 0666 & ~0$old_umask )))" "$copy" || exit 1
fi"#
    )
}

/// Anchors the target's folder with `cd -P`, so no later step re-resolves
/// the folder's path, and copies stdin to a file `mktemp` makes in a private
/// work folder made inside it, where no one else can reach the copy; every
/// file the push writes there is one it just made, so a folder swapped in
/// for the work folder loses nothing. Refuses the copy unless it holds
/// exactly `len` bytes, checks `expect`, gives the copy the access of the
/// target or of `access`'s source, and renames it over the target, so the
/// file becomes the SSH user's. A push expecting nothing at the path checks
/// again just before the rename. When that access cannot be kept, it writes
/// over an existing target in place instead (see [`keep_access`]). The trap
/// removes the copy and the work folder on every exit.
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
    let absent_before_rename = match expect {
        Expect::Absent => absent_check(),
        Expect::Sha256(_) | Expect::Any => String::new(),
    };
    format!(
        r#"old_umask=$(umask)
umask 077
{assign}
{source}
{refuse_path}
{hasher}
case $path in
*/*) dir=${{path%/*}} ;;
*) dir=. ;;
esac
if [ -z "$dir" ]; then dir=/; fi
name=${{path##*/}}
case $source in ''|/*) ;; *) source=$PWD/$source ;; esac
uid=$(id -u 2>/dev/null) || exit 1
case $uid in ''|*[!0-9]*) exit 1 ;; esac
{INODE_FUNCTION}
{FOLDER_IS_SAFE_FUNCTION}
{CLEANUP_FUNCTION}
cd -P -- "$dir" || exit 1
here=$(inode .)
if [ -z "$here" ]; then exit 1; fi
if folder_is_safe; then safe=1; else safe=; fi
work=
inside=
copy=
probe=
probe_to=
trap cleanup EXIT
work=$(mktemp -d ./.herdr-push.XXXXXX) || exit 1
{enter}
target=../$name
copy=$(mktemp ./f.XXXXXX) || exit 1
cat > "$copy" || exit 1
size=$(wc -c < "$copy") || exit 1
case $size in ''|*[!0-9\ ]*) exit 1 ;; esac
if [ $size -ne {len} ]; then exit {SHORT_EXIT}; fi
{refuse_target}
{check}
{keep}
if [ "$(inode ..)" != "$here" ]; then exit {SHARED_FOLDER_EXIT}; fi
{PICK_NO_FOLLOW}
{absent_before_rename}
mv -f $no_follow -- "$copy" "$target" || exit 1
copy="#,
        assign = path_assignment(path),
        refuse_path = refuse_unsafe("path"),
        refuse_target = refuse_unsafe("target"),
        check = expect_check(expect),
        enter = enter_work_folder(),
        keep = keep_access(expect),
    )
}
