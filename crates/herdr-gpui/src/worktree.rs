//! Client-side previews of the daemon's worktree naming, so the new worktree
//! dialog can propose the branch and show the checkout it is about to ask for,
//! as the terminal client does. The daemon still derives the authoritative
//! branch and path; nothing here decides them. A branch proposed from the
//! workspace's name is sent like any typed branch.

/// Namespace the daemon gives every branch it generates for a new checkout.
const PREFIX: &str = "worktree";
/// Longest slug this client proposes for a branch, so a long name or issue
/// title cannot produce an unusable checkout directory.
const SLUG_LIMIT: usize = 60;

/// Validate a literal branch name, without resolving checkout shorthand or
/// consulting a repository (which may live on a remote endpoint).
pub(super) fn validate_branch(branch: &str) -> crate::Result<()> {
    let invalid = branch.is_empty()
        || branch == "@"
        || branch == "HEAD"
        || branch.starts_with('-')
        || branch.ends_with('.')
        || branch.contains("..")
        || branch.contains("@{")
        || branch
            .bytes()
            .any(|byte| byte <= b' ' || byte == 0x7f || b"~^:?*[\\".contains(&byte))
        || branch
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"));
    if invalid {
        return Err(crate::Error::InvalidBranchName);
    }
    Ok(())
}

/// The branch the daemon would generate for `seed`. Pure, so the proposal is
/// reproducible in tests; callers supply the clock.
pub(super) fn generated_branch_slug(seed: u64) -> String {
    const ADJECTIVES: [&str; 8] = [
        "brave", "calm", "clear", "green", "lucky", "quiet", "rapid", "silver",
    ];
    const NOUNS: [&str; 8] = [
        "river", "cloud", "field", "forest", "harbor", "meadow", "stone", "valley",
    ];
    let adjective = ADJECTIVES[(seed as usize) % ADJECTIVES.len()];
    let noun = NOUNS[((seed / ADJECTIVES.len() as u64) as usize) % NOUNS.len()];
    let suffix = seed & 0xffff;
    format!("{PREFIX}/{adjective}-{noun}-{suffix:04x}")
}

/// A branch proposal seeded from the current clock, as the daemon seeds its own.
pub(super) fn proposed_branch() -> String {
    generated_branch_slug(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_micros().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0),
    )
}

/// The daemon's slug of `text`, cut to `limit` bytes: lowercase ASCII letters
/// and digits, with every other run of characters collapsed to one dash and
/// none at either end. Empty when nothing survives. Reading stops at the
/// limit, so a long text costs no more than a short one; a leading run is
/// never written, so it does not count towards the limit.
fn slug(text: &str, limit: usize) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in text.chars() {
        if slug.len() >= limit {
            break;
        }
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    let kept = slug.trim_end_matches('-').len();
    slug.truncate(kept);
    slug
}

/// The directory component the daemon derives from a branch name.
pub(super) fn branch_to_path_slug(branch: &str) -> String {
    let slug = slug(branch, usize::MAX);
    if slug.is_empty() {
        PREFIX.to_owned()
    } else {
        slug
    }
}

/// `text` slugged for a proposed branch: the daemon's slug, cut to
/// [`SLUG_LIMIT`] without leaving a trailing dash. Empty when nothing survives.
pub(super) fn bounded_slug(text: &str) -> String {
    slug(text, SLUG_LIMIT)
}

/// The branch proposed for a workspace named `name`, in the daemon's
/// namespace. Only ASCII letters and digits survive, because the daemon names
/// the checkout folder from the branch the same way: a branch kept in another
/// script would get the folder `worktree` whatever its name, and two of them
/// would collide. `None` when nothing survives, so the caller keeps its own
/// proposal.
pub(super) fn branch_for_name(name: &str) -> Option<String> {
    let slug = bounded_slug(name);
    (!slug.is_empty()).then(|| format!("{PREFIX}/{slug}"))
}

/// The checkout the daemon would create under its worktree directory. This is an
/// endpoint path, not a path on the machine drawing the dialog, so the separator
/// follows the root the daemon reported rather than this platform's.
pub(super) fn checkout_preview(root: &str, repo: &str, branch: &str) -> String {
    let separator = if !root.starts_with('/') && root.contains('\\') {
        '\\'
    } else {
        '/'
    };
    format!(
        "{}{separator}{repo}{separator}{}",
        root.trim_end_matches(separator),
        branch_to_path_slug(branch)
    )
}

/// The main checkout a `<root>/.git` Git directory belongs to. Any other key
/// (a bare repository, a separate Git directory) does not name its checkout.
pub(super) fn checkout_root(key: &str) -> Option<&str> {
    key.strip_suffix("/.git")
        .or_else(|| key.strip_suffix("\\.git"))
        .filter(|root| !root.is_empty())
}

/// Whether `cwd` lies outside the main checkout whose Git directory is `key`.
/// Herdr keeps the repository a workspace was opened in while its directory,
/// and the label drawn from it, follow the shell, so a workspace whose shell
/// moved into another repository still creates worktrees of the first. Only a
/// `<root>/.git` key names its checkout; any other key, or no directory, is
/// not known to have left. These are endpoint paths, so either separator is
/// accepted and a Windows path ignores case.
pub(super) fn left_checkout(key: &str, cwd: &str) -> bool {
    let Some(root) = checkout_root(key) else {
        return false;
    };
    if cwd.is_empty() {
        return false;
    }
    let windows = !root.starts_with('/');
    let normalize = |path: &str| {
        let path = path.trim_end_matches(['/', '\\']);
        if windows {
            path.replace('\\', "/").to_lowercase()
        } else {
            path.to_owned()
        }
    };
    let (root, cwd) = (normalize(root), normalize(cwd));
    let inside = cwd == root
        || cwd
            .strip_prefix(&root)
            .is_some_and(|rest| rest.starts_with('/'));
    !inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_branch_names_follow_git_ref_rules() {
        for branch in [
            "config reload",
            "feature\tbranch",
            "feature\nbranch",
            "bad\u{7f}",
            "",
            "@",
            "HEAD",
            "-option",
            "a..b",
            "a@{b",
            "a~b",
            "a^b",
            "a:b",
            "a?b",
            "a*b",
            "a[b",
            "a\\b",
            "/a",
            "a/",
            "a//b",
            ".hidden",
            "a/.b",
            "a.lock",
            "a.lock/b",
            "a.",
        ] {
            assert!(
                matches!(
                    validate_branch(branch),
                    Err(crate::Error::InvalidBranchName)
                ),
                "{branch:?}"
            );
        }
        for branch in [
            "config-reload",
            "feature/test",
            "fix.v2",
            "a./b",
            "日本語",
            "a@b",
        ] {
            assert!(validate_branch(branch).is_ok(), "{branch:?}");
        }
    }

    #[test]
    fn generated_branches_are_namespaced_and_reproducible() {
        assert_eq!(generated_branch_slug(0), "worktree/brave-river-0000");
        assert_eq!(generated_branch_slug(9), "worktree/calm-cloud-0009");
        assert_eq!(
            generated_branch_slug(u64::MAX),
            "worktree/silver-valley-ffff"
        );
        let proposed = proposed_branch();
        assert!(proposed.starts_with("worktree/"), "{proposed}");
        assert_eq!(proposed.split('-').count(), 3, "{proposed}");
    }

    #[test]
    fn checkout_previews_follow_the_endpoint_root_and_branch_slug() {
        assert_eq!(
            checkout_preview(
                "/home/me/.herdr/worktrees/",
                "herdr",
                "worktree/brave-river"
            ),
            "/home/me/.herdr/worktrees/herdr/worktree-brave-river"
        );
        assert_eq!(
            checkout_preview("C:\\Users\\me\\worktrees", "herdr", "feature/Login v2"),
            "C:\\Users\\me\\worktrees\\herdr\\feature-login-v2"
        );
        // A branch with nothing to slug still lands in the namespaced directory.
        assert_eq!(branch_to_path_slug("///"), "worktree");
        assert_eq!(branch_to_path_slug("--Fix_42--"), "fix-42");
    }

    #[test]
    fn a_name_proposes_a_namespaced_branch_that_matches_its_folder() {
        assert_eq!(
            branch_for_name("  Fix login! "),
            Some("worktree/fix-login".into())
        );
        // A name that is already a branch stays inside the namespace.
        assert_eq!(branch_for_name("main"), Some("worktree/main".into()));
        assert_eq!(
            branch_for_name("feat/Login v2"),
            Some("worktree/feat-login-v2".into())
        );
        // Nothing to slug leaves the proposal to the caller.
        for name in ["", "   ", "---", "\u{65e5}\u{672c}\u{8a9e}", "\u{1f600}"] {
            assert_eq!(branch_for_name(name), None, "{name:?}");
        }
        // The cut never leaves a trailing dash, and the result is always a
        // branch the dialog would accept.
        assert_eq!(
            branch_for_name(&"word ".repeat(40)),
            Some(format!("worktree/{}word", "word-".repeat(11)))
        );
        // Separators before the first letter are not written, so they leave
        // the whole limit to the name.
        assert_eq!(
            branch_for_name(&format!("{}{}", "- ".repeat(100), "a".repeat(100))),
            Some(format!("worktree/{}", "a".repeat(60)))
        );
        for name in [
            "Fix login",
            "a.lock",
            "a..b",
            "@{x}",
            "HEAD",
            &"x".repeat(200),
        ] {
            let branch = branch_for_name(name).unwrap_or_default();
            assert!(validate_branch(&branch).is_ok(), "{branch}");
            assert_eq!(
                branch_to_path_slug(&branch),
                branch.replace('/', "-"),
                "{branch}"
            );
        }
    }
}
