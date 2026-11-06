//! Daemon worktree keys and the checkouts they name.

/// The main checkout a `<root>/.git` Git directory belongs to. Any other key
/// (a bare repository, a separate Git directory) does not name its checkout.
pub fn checkout_root(key: &str) -> Option<&str> {
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
pub fn left_checkout(key: &str, cwd: &str) -> bool {
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
