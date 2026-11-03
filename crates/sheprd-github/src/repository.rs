//! GitHub.com repositories named by Git remote URLs.

/// A bound on each remote path segment, above GitHub's own name limits.
const MAX_SEGMENT: usize = 100;

/// One segment of a remote's path.
///
/// Account and repository names are GitHub's to define, so no name grammar is
/// reproduced here: a segment is refused only when it names the directory
/// itself or carries punctuation that would not stay inside its own path
/// segment in the avatar URL built from it.
fn segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SEGMENT
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// The lowercased owner and the repository a GitHub.com remote URL names.
pub fn github_repo(remote: &str) -> Option<(String, String)> {
    let path = ["git@github.com:", "ssh://git@github.com/"]
        .into_iter()
        .find_map(|prefix| remote.strip_prefix(prefix))
        .or_else(|| http_path(remote))?;
    let (owner, repo) = path.split_once('/')?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if !segment(owner) || !segment(repo) {
        return None;
    }
    Some((owner.to_ascii_lowercase(), repo.to_owned()))
}

/// The path of an `http(s)://[userinfo@]github.com/` remote.
///
/// Git accepts a user name, or a user name and token, before the host
/// (`https://user@github.com/o/r`, `https://user:token@github.com/o/r`).
/// The host is what Git connects to, so it alone must be `github.com`; the
/// userinfo is skipped and never kept. Userinfo that a URL parser could read
/// as the end of the authority (`/`, `?`, `#`, `\`, a second `@`) is refused
/// so the host cannot be misread.
fn http_path(remote: &str) -> Option<&str> {
    let rest = ["https://", "http://"]
        .into_iter()
        .find_map(|scheme| remote.strip_prefix(scheme))?;
    let (authority, path) = rest.split_once('/')?;
    let host = match authority.split_once('@') {
        None => authority,
        Some((userinfo, host)) => {
            let plain = |b: u8| b.is_ascii_graphic() && !matches!(b, b'@' | b'?' | b'#' | b'\\');
            if userinfo.is_empty() || !userinfo.bytes().all(plain) {
                return None;
            }
            host
        }
    };
    (host == "github.com").then_some(path)
}
