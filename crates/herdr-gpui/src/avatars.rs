use gpui::{Image, ImageFormat};
use std::{
    collections::HashMap,
    process::Command,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

/// Session-local avatar cache. Git and HTTP work never run on the caller's thread.
pub struct Avatars {
    requests: mpsc::Sender<String>,
    results: mpsc::Receiver<(String, Option<Arc<Image>>)>,
    // Presence deduplicates pending requests as well as resolved hits and misses.
    images: HashMap<String, Option<Arc<Image>>>,
}

impl Avatars {
    pub fn new() -> Self {
        let (requests, incoming) = mpsc::channel::<String>();
        let (outgoing, results) = mpsc::channel();
        // Deliberately detach: dropping the sender ends the worker without a UI join.
        thread::spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .max_redirects(0)
                .build()
                .into();
            let mut owners = HashMap::new();
            for cwd in incoming {
                let image = repo_owner(&cwd).and_then(|owner| {
                    owners
                        .entry(owner.clone())
                        .or_insert_with(|| fetch_avatar(&agent, &owner))
                        .clone()
                });
                if outgoing.send((cwd, image)).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            results,
            images: HashMap::new(),
        }
    }

    /// Enqueue each exact cwd at most once, including failed resolutions.
    pub fn request(&mut self, cwd: &str) {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.images.entry(cwd.to_owned())
        {
            let _ = self.requests.send(entry.key().clone());
            entry.insert(None);
        }
    }

    /// Drain completed requests without blocking. Misses also count as updates.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for (cwd, image) in self.results.try_iter() {
            self.images.insert(cwd, image);
            changed = true;
        }
        changed
    }

    pub fn image(&self, cwd: &str) -> Option<Arc<Image>> {
        self.images.get(cwd).cloned().flatten()
    }
}

fn repo_owner(cwd: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", cwd, "config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // Git resolves both normal repositories and linked worktrees for us.
    github_owner(
        std::str::from_utf8(&output.stdout)
            .ok()?
            .trim_end_matches(['\r', '\n']),
    )
}

fn github_owner(remote: &str) -> Option<String> {
    let path = [
        "https://github.com/",
        "http://github.com/",
        "git@github.com:",
        "ssh://git@github.com/",
    ]
    .into_iter()
    .find_map(|prefix| remote.strip_prefix(prefix))?;
    let (owner, repo) = path.split_once('/')?;
    // GitHub usernames: 1-39 ASCII alphanumerics or single internal hyphens.
    if owner.is_empty()
        || owner.len() > 39
        || owner.starts_with('-')
        || owner.ends_with('-')
        || owner.contains("--")
        || !owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if repo.is_empty()
        || repo == "."
        || repo == ".."
        || !repo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return None;
    }
    Some(owner.to_ascii_lowercase())
}

fn fetch_avatar(agent: &ureq::Agent, owner: &str) -> Option<Arc<Image>> {
    // Only called with an owner validated by github_owner, never a remote URL.
    let url = format!("https://avatars.githubusercontent.com/{owner}?size=48");
    let mut response = agent.get(&url).call().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let content_type = response.headers().get("content-type")?.to_str().ok()?;
    let format = avatar_format(content_type)?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(1_000_000)
        .read_to_vec()
        .ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

fn avatar_format(content_type: &str) -> Option<ImageFormat> {
    match content_type
        .split(';')
        .next()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::Webp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    #[ignore = "requires HERDR_AVATAR_TEST_REPO and network access to GitHub avatars"]
    fn loads_local_repository_owner_avatar() {
        let cwd =
            std::env::var("HERDR_AVATAR_TEST_REPO").expect("set a local GitHub repository path");
        let owner = repo_owner(&cwd).expect("repository has a GitHub origin");
        let mut avatars = Avatars::new();
        avatars.request(&cwd);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !avatars.poll() {
            assert!(
                std::time::Instant::now() < deadline,
                "avatar worker timed out"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let image = avatars.image(&cwd).expect("owner avatar downloaded");
        assert!(!image.bytes.is_empty());
        eprintln!("Avatar loaded for {owner}: {} bytes", image.bytes.len());
    }

    #[test]
    fn parses_github_remotes() {
        for prefix in [
            "https://github.com/",
            "http://github.com/",
            "git@github.com:",
            "ssh://git@github.com/",
        ] {
            for repo in ["repo", "repo.git", "a_repo-1.2.git"] {
                assert_eq!(
                    github_owner(&format!("{prefix}Some-Owner/{repo}")),
                    Some("some-owner".to_owned())
                );
            }
        }
        assert_eq!(github_owner("https://github.com/a/r"), Some("a".into()));
    }

    #[test]
    fn rejects_untrusted_hosts_and_malformed_paths() {
        for remote in [
            "https://github.com.evil.test/owner/repo",
            "https://github.com@evil.test/owner/repo",
            "https://evil.test/github.com/owner/repo",
            "https://github.com:443/owner/repo",
            "git@github.com.evil.test:owner/repo",
            "ssh://git@github.com.evil.test/owner/repo",
            "ssh://git@github.com:22/owner/repo",
            "file:///github.com/owner/repo",
            "https://github.com/owner",
            "https://github.com/owner/",
            "https://github.com/owner/.git",
            "https://github.com/owner/..",
            "https://github.com/owner/repo/extra",
            "https://github.com/owner/repo/",
            "https://github.com/owner/repo?query",
            "https://github.com/owner/repo#fragment",
            "https://github.com/owner/repo%2fextra",
            "https://github.com/owner/repo\\extra",
            "https://github.com/owner/repo\n",
        ] {
            assert_eq!(github_owner(remote), None, "{remote}");
        }
        for owner in [
            "", "-owner", "owner-", "a--b", "a_b", "a.b", "a@b", "a%2fb", "a b", "\u{e9}",
        ] {
            assert_eq!(
                github_owner(&format!("https://github.com/{owner}/repo")),
                None
            );
        }
        assert!(github_owner(&format!("https://github.com/{}/repo", "a".repeat(39))).is_some());
        assert!(github_owner(&format!("https://github.com/{}/repo", "a".repeat(40))).is_none());
    }

    #[test]
    fn accepts_only_supported_content_types() {
        for (mime, format) in [
            ("image/png", ImageFormat::Png),
            ("IMAGE/JPEG; charset=binary", ImageFormat::Jpeg),
            ("image/gif", ImageFormat::Gif),
            ("image/webp", ImageFormat::Webp),
        ] {
            assert_eq!(avatar_format(mime), Some(format));
        }
        for mime in [
            "",
            "text/html",
            "image/svg+xml",
            "image/png-extra",
            "application/octet-stream",
        ] {
            assert_eq!(avatar_format(mime), None);
        }
    }

    #[test]
    fn deduplicates_pending_hits_and_misses_and_drains_results() {
        let (requests, incoming) = mpsc::channel();
        let (outgoing, results) = mpsc::channel();
        let mut avatars = Avatars {
            requests,
            results,
            images: HashMap::new(),
        };
        assert!(!avatars.poll());
        assert!(avatars.image("hit").is_none());
        for cwd in ["hit", "miss", "hit", "miss"] {
            avatars.request(cwd);
        }
        assert_eq!(incoming.try_iter().collect::<Vec<_>>(), ["hit", "miss"]);
        let image = Arc::new(Image::empty());
        assert!(outgoing.send(("hit".into(), Some(image.clone()))).is_ok());
        assert!(outgoing.send(("miss".into(), None)).is_ok());
        assert!(avatars.poll());
        assert!(!avatars.poll());
        assert!(
            avatars
                .image("hit")
                .is_some_and(|cached| Arc::ptr_eq(&cached, &image))
        );
        assert!(avatars.image("miss").is_none());
        avatars.request("hit");
        avatars.request("miss");
        assert!(incoming.try_recv().is_err());
    }
}
