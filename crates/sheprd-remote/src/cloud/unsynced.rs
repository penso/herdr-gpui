//! Work a cloud machine holds that exists nowhere else, looked for before the
//! machine is deleted with everything on it. Teleport and dispatch put
//! checkouts under the home directory (`~/code/app`, or Herdr's own worktree
//! directory), so a bounded search there finds them; each one with
//! uncommitted files or commits on no remote is reported, and the Remove
//! dialog asks again before deleting.
//!
//! The search runs through the device's own command, like Teleport's scripts,
//! on a worker. It never changes anything on the machine.

use sheprd_client::ConnectTarget;
use std::sync::atomic::AtomicBool;

/// The most checkouts reported; the dialog only names the first few.
const MAX_CHECKOUTS: usize = 200;

/// A checkout on the machine with work that would be lost with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkout {
    /// The checkout's path under the machine's home directory, as `~/…`.
    pub path: String,
    /// Changed, staged, and untracked (not ignored) files.
    pub changed: u32,
    /// Commits reachable from local refs or HEAD but from no remote-tracking branch.
    pub unpushed: u32,
}

/// Toolchains, package caches, and build output hold Git checkouts the user
/// never made (Cargo's Git dependencies have no remote, so every commit would
/// count as unpushed); the search skips them.
const SKIPPED: &[&str] = &[
    ".bun",
    ".cache",
    ".cargo",
    ".gradle",
    ".local",
    ".m2",
    ".npm",
    ".nvm",
    ".rustup",
    "node_modules",
    "target",
];

fn script() -> String {
    let skipped = SKIPPED
        .iter()
        .map(|name| format!("-name {name}"))
        .chain(
            // Go's module cache, not its source tree: `~/go/src` holds the
            // user's own checkouts.
            ["-path ./go/pkg".to_owned()],
        )
        .collect::<Vec<_>>()
        .join(" -o ");
    format!(
        r#"cd "$HOME" || exit 1
find . -maxdepth 6 \( {skipped} \) -prune -o -name .git -print 2>/dev/null | head -n {MAX_CHECKOUTS} |
while IFS= read -r git; do
  dir=${{git%/.git}}
  status=$(git -C "$dir" status --porcelain=v1 --untracked-files=all) || exit 1
  changed=0
  if [ -n "$status" ]; then
    changed=$(printf '%s\n' "$status" | wc -l | tr -d ' ')
  fi
  unpushed=$(git -C "$dir" rev-list --count --all --not --remotes) || exit 1
  if [ "$changed" != 0 ] || [ "$unpushed" != 0 ]; then
    printf '%s\t%s\t%s\n' "$changed" "$unpushed" "$dir"
  fi
done
"#
    )
}

fn parse(output: &str) -> Vec<Checkout> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let changed = fields.next()?.trim().parse().ok()?;
            let unpushed = fields.next()?.trim().parse().ok()?;
            let dir = fields.next()?.strip_prefix("./")?;
            Some(Checkout {
                path: format!("~/{dir}"),
                changed,
                unpushed,
            })
        })
        .take(MAX_CHECKOUTS)
        .collect()
}

/// The checkouts on the machine `target` reaches that have work found
/// nowhere else. Blocks: run it on a worker.
pub fn find(target: &ConnectTarget, cancelled: &AtomicBool) -> crate::Result<Vec<Checkout>> {
    // Window tests must not reach a provider; they say what a machine holds.
    #[cfg(any(test, feature = "test-support"))]
    if let ConnectTarget::Cloud { id, .. } = target {
        return FAKE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .and_then(|machines| machines.get(id).cloned())
            .ok_or(crate::Error::Client(
                sheprd_client::Error::ScriptUnsupported,
            ));
    }
    let host = crate::teleport::Host::new(target)?;
    let output = host.capture(&script(), cancelled)?;
    Ok(parse(&String::from_utf8_lossy(&output)))
}

/// What `find` reports for each cloud machine ID in window tests; a machine
/// missing here cannot be searched.
#[cfg(any(test, feature = "test-support"))]
pub static FAKE: std::sync::Mutex<Option<std::collections::HashMap<String, Vec<Checkout>>>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) mod tests;
