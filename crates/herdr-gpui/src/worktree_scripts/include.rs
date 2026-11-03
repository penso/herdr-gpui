//! Copies the ignored files a repository's `.worktreeinclude` names from its
//! main checkout into a linked worktree, before that worktree's scripts are
//! read, so a personal `.herdr/worktree.toml` or `.env` reaches it.
//!
//! The file uses `.gitignore` syntax and is read from the main checkout, so it
//! can itself be ignored. Git on the daemon's host decides what matches: a
//! file is copied only when it is untracked, matches `.worktreeinclude`, and
//! is ignored by Git there, so tracked files are never duplicated. A file the
//! worktree already has is never overwritten, a symbolic link is copied as a
//! link, and nothing is written below a directory of the worktree that leads
//! outside it.

use super::{Checkout, Error};
use herdr_client::{ConnectTarget, ScriptHost, ScriptLimits, run_script, shell_quote};
use std::{sync::atomic::AtomicBool, time::Duration};

/// The list, relative to the main checkout; the script names it itself.
#[cfg(any(windows, test))]
const PATH: &str = ".worktreeinclude";
/// One line is printed per copied file; this bounds how many are copied.
const MAX_OUTPUT: u64 = 1024 * 1024;
const IDLE: Duration = Duration::from_secs(60);

/// Lists with Git in the main checkout `$r` and copies into `$w`. Each copy
/// prints a line, so a long run of copies is not mistaken for a stall.
///
/// As checkpoints' scripts do, it finds a Git installed outside the minimal
/// `PATH` an app or a non-interactive SSH shell starts with, and turns off
/// `core.fsmonitor`, a command the repository's configuration would
/// otherwise have Git run.
///
/// The lists pass through temporary files rather than a pipeline, whose
/// status would be the last command's alone, so a failing Git command fails
/// the copy; `check-ignore` exits 1 when nothing is ignored. Each file is
/// copied into a private directory beside its destination and renamed into
/// place, so a copy that fails partway leaves no partial file for every
/// later run to skip, and no one else can open a file before its mode is
/// final.
///
/// The copy runs under `umask 077`: a directory it creates is private, as
/// one around a secret in the main checkout may be, and so is a file while
/// it is written; `cp -p` then gives each file its own mode. A file the main
/// checkout's directories keep from the group or other users loses that
/// access when the worktree's would give it: `reach` asks whether that class
/// can search every directory from one up to `/`, by the execute bit `ls -ld`
/// shows in column 7 for the group and 10 for others. A symbolic link is
/// left as it is, since `chmod` would change what it points to.
const BODY: &str = r#"set -eu
[ -f "$r/.worktreeinclude" ] || exit 0
herdr_linuxbrew=
case "$(uname -s)" in Linux) herdr_linuxbrew=/home/linuxbrew/.linuxbrew/bin:;; esac
PATH="$PATH:/opt/homebrew/bin:/usr/local/bin:$herdr_linuxbrew$HOME/.nix-profile/bin"
export PATH
g() { git -c core.fsmonitor=false "$@"; }
w=$(cd -P -- "$w" && pwd -P)
cd -- "$r"
listed=$(mktemp "${TMPDIR:-/tmp}/herdr-worktreeinclude.XXXXXX")
ignored=$(mktemp "${TMPDIR:-/tmp}/herdr-worktreeinclude.XXXXXX")
trap 'rm -f "$listed" "$ignored"' EXIT
g ls-files -z --others --ignored --exclude-from=.worktreeinclude >"$listed"
g check-ignore -z --stdin <"$listed" >"$ignored" || [ $? -eq 1 ]
W=$w xargs -0 sh -ec '
umask 077
reach() {
  p=$2
  while :; do
    case $(ls -ld -- "$p" | cut -c"$1") in x|s|t) ;; *) return 1 ;; esac
    [ "$p" = / ] && return 0
    p=$(dirname -- "$p")
  done
}
for f do
  if [ -e "$W/$f" ] || [ -L "$W/$f" ]; then continue; fi
  if [ ! -f "$f" ] && [ ! -L "$f" ]; then continue; fi
  d=$(dirname -- "$f")
  a=$d
  while [ "$a" != . ] && [ ! -e "$W/$a" ] && [ ! -L "$W/$a" ]; do a=$(dirname -- "$a"); done
  real=$(cd -P -- "$W/$a" 2>/dev/null && pwd -P) || continue
  case $real/ in "$W"/*) ;; *) continue ;; esac
  mkdir -p -- "$W/$d"
  s=$W/$d/.worktreeinclude.$$
  t=$s/file
  if [ -d "$s" ] && [ ! -L "$s" ]; then rm -f -- "$t"; rmdir -- "$s"; fi
  mkdir -- "$s"
  if ! cp -RPp -- "$f" "$t"; then rm -f -- "$t"; rmdir -- "$s"; exit 1; fi
  if [ ! -L "$t" ]; then
    from=$(cd -P -- "$d" && pwd -P)
    into=$(cd -P -- "$W/$d" && pwd -P)
    if ! reach 7 "$from" && reach 7 "$into"; then chmod g-rwx "$t"; fi
    if ! reach 10 "$from" && reach 10 "$into"; then chmod o-rwx "$t"; fi
  fi
  if [ -e "$W/$f" ] || [ -L "$W/$f" ]; then rm -f -- "$t"; rmdir -- "$s"; continue; fi
  mv -f -- "$t" "$W/$f"
  rmdir -- "$s"
  printf "\n"
done' sh <"$ignored"
"#;

/// Copy what the main checkout's `.worktreeinclude` names into `checkout`, on
/// `target`'s host. Nothing to do for a checkout that is the main one, or
/// whose main checkout the daemon did not name. Blocking: call only on a
/// background executor.
///
/// Never cancelled: a copy killed partway could leave a truncated file that
/// is then never overwritten. The idle limit ends a stalled one.
pub(crate) fn copy(target: &ConnectTarget, checkout: &Checkout) -> crate::Result<()> {
    let Some(root) = checkout
        .root
        .as_deref()
        .filter(|root| *root != checkout.path)
    else {
        return Ok(());
    };
    match target {
        ConnectTarget::Ssh { target, .. } => run(ScriptHost::Ssh(target), root, &checkout.path),
        ConnectTarget::Local | ConnectTarget::Session { .. } | ConnectTarget::Socket(_) => {
            local(root, &checkout.path)
        }
        // Scripts are not read there either; the read reports it.
        ConnectTarget::Wsl { .. } => Err(Error::UnsupportedHost.into()),
        #[cfg(feature = "cloud")]
        ConnectTarget::Cloud { .. } => Err(Error::UnsupportedHost.into()),
    }
}

#[cfg(not(windows))]
fn local(root: &str, checkout: &str) -> crate::Result<()> {
    run(ScriptHost::Local, root, checkout)
}

/// Says so only when the repository has a list to copy from.
#[cfg(windows)]
fn local(root: &str, _: &str) -> crate::Result<()> {
    if std::path::Path::new(root).join(PATH).is_file() {
        return Err(Error::CopyUnsupported.into());
    }
    Ok(())
}

fn run(host: ScriptHost<'_>, root: &str, checkout: &str) -> crate::Result<()> {
    run_script(
        host,
        &script(root, checkout),
        std::io::empty(),
        std::io::sink(),
        ScriptLimits {
            output: MAX_OUTPUT,
            idle: IDLE,
        },
        &AtomicBool::new(false),
    )
    .map_err(Error::Copy)?;
    Ok(())
}

/// The copy from `root` into `checkout`, with both paths quoted.
fn script(root: &str, checkout: &str) -> String {
    format!(
        "r={} w={}\n{BODY}",
        shell_quote(root),
        shell_quote(checkout)
    )
}

#[cfg(test)]
mod tests;
