//! The branches a branch review may be compared with, for the user to pick
//! from: local and remote-tracking branches, the ones the checkout left most
//! recently first, so the branch it was cut from is near the top.
use super::load::{branch_ref, short_ref};
use crate::pull_request::Input;
use std::time::{Duration, Instant};

const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// Output of the listing: a ref name and two counts per branch.
const LIST_OUTPUT: usize = 4 * 1024 * 1024;
/// Branches offered at most; past the closest, a long tail helps no one.
pub(crate) const MAX_BASES: usize = 200;

/// A branch the review may be compared with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BaseChoice {
    /// The full ref, as the review asks for it.
    pub reference: String,
    /// How many of the checkout's commits the branch lacks: the fewer, the
    /// closer the checkout's start. None when Git cannot count them.
    pub behind: Option<u64>,
}

impl BaseChoice {
    /// The branch as it is shown: `origin/main` or `main`.
    pub(crate) fn label(&self) -> &str {
        short_ref(&self.reference)
    }
}

/// Branches of the checkout to compare `input`'s branch with, closest
/// first; the branch itself and its remote copies are left out. Reads local
/// refs only; nothing is fetched. Blocking.
pub(crate) fn bases(input: &Input) -> crate::Result<Vec<BaseChoice>> {
    let deadline = Instant::now() + LIST_TIMEOUT;
    let never = || false;
    let checkout = crate::pull_request::local_checkout(input, deadline, &never)?;
    let list = |format: &str| {
        crate::git::git_bytes(
            &checkout,
            &[
                "for-each-ref",
                &format!("--format={format}"),
                "refs/heads",
                "refs/remotes",
            ],
            "list branches", // i18n: data
            deadline,
            &never,
            LIST_OUTPUT,
        )
    };
    // `ahead-behind` needs Git 2.41; an older one lists the names alone.
    let output = list("%(refname) %(ahead-behind:HEAD)").or_else(|_| list("%(refname)"))?;
    Ok(parse_bases(
        &String::from_utf8_lossy(&output),
        &input.branch,
    ))
}

/// `for-each-ref` lines, each a ref name and optionally its ahead and behind
/// counts against HEAD, as the choices to offer for `branch`: closest first,
/// then by name, at most [`MAX_BASES`].
pub(super) fn parse_bases(output: &str, branch: &str) -> Vec<BaseChoice> {
    let mut choices: Vec<BaseChoice> = output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let reference = fields.next()?;
            let behind = fields.nth(1).and_then(|count| count.parse().ok());
            Some(BaseChoice {
                reference: reference.to_owned(),
                behind,
            })
        })
        .filter(|choice| offered(&choice.reference, branch))
        .collect();
    choices.sort_by(|a, b| {
        (a.behind.is_none(), a.behind, a.label()).cmp(&(b.behind.is_none(), b.behind, b.label()))
    });
    choices.truncate(MAX_BASES);
    choices
}

/// Whether `reference` may be offered as the base of `branch`: a plain
/// branch other than `branch` itself, its copy on a remote, or a remote's
/// `HEAD`, which only points at another branch listed anyway.
fn offered(reference: &str, branch: &str) -> bool {
    if !branch_ref(reference) {
        return false;
    }
    if let Some(local) = reference.strip_prefix("refs/heads/") {
        return local != branch;
    }
    let remote = reference.strip_prefix("refs/remotes/").unwrap_or(reference);
    remote
        .split_once('/')
        .is_some_and(|(_, name)| name != branch && name != "HEAD")
}
