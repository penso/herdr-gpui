use gpui_kit::{Image, ImageFormat};
mod cache;
use std::{
    collections::HashMap,
    process::Command,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, SystemTime},
};

const MAX_REQUESTS: usize = 128;

/// A bound on each remote path segment, above GitHub's own name limits.
const MAX_SEGMENT: usize = 100;

/// Memory front end to the shared public-image disk cache.
pub struct Avatars {
    requests: mpsc::SyncSender<String>,
    results: mpsc::Receiver<(String, Option<Arc<Image>>)>,
    // Presence deduplicates pending requests as well as resolved hits and misses.
    images: HashMap<String, Option<Arc<Image>>>,
}

impl Avatars {
    pub fn new() -> Self {
        let (requests, incoming) = mpsc::sync_channel::<String>(MAX_REQUESTS);
        let (outgoing, results) = mpsc::sync_channel(MAX_REQUESTS * 2);
        let (refresh, network) = mpsc::sync_channel::<(String, String)>(MAX_REQUESTS);
        let updated = outgoing.clone();
        thread::spawn(move || {
            let agent = avatar_agent();
            let mut owners = HashMap::new();
            for (cwd, url) in network {
                let image = owners
                    .entry(url.clone())
                    .or_insert_with(|| download(&agent, &url))
                    .clone();
                if let Some(image) = image
                    && updated.send((cwd, Some(image))).is_err()
                {
                    break;
                }
            }
        });
        // Deliberately detach: dropping the sender ends the worker without a UI join.
        thread::spawn(move || {
            for cwd in incoming {
                let url = repo_owner(&cwd)
                    .map(|owner| format!("https://avatars.githubusercontent.com/{owner}?size=48"));
                let cached = url.as_deref().and_then(cached_image);
                let fresh = cached.as_ref().is_some_and(|(_, fresh)| *fresh);
                if outgoing
                    .send((cwd.clone(), cached.map(|(image, _)| image)))
                    .is_err()
                {
                    break;
                }
                if !fresh && let Some(url) = url {
                    let _ = refresh.try_send((cwd, url));
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
        if self.images.len() >= MAX_REQUESTS {
            return;
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = self.images.entry(cwd.to_owned())
            && self.requests.try_send(entry.key().clone()).is_ok()
        {
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
    github_repo(remote).map(|(owner, _)| owner)
}

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

pub(crate) fn github_repo(remote: &str) -> Option<(String, String)> {
    let path = [
        "https://github.com/",
        "http://github.com/",
        "git@github.com:",
        "ssh://git@github.com/",
    ]
    .into_iter()
    .find_map(|prefix| remote.strip_prefix(prefix))?;
    let (owner, repo) = path.split_once('/')?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if !segment(owner) || !segment(repo) {
        return None;
    }
    Some((owner.to_ascii_lowercase(), repo.to_owned()))
}

fn cached_image(url: &str) -> Option<(Arc<Image>, bool)> {
    if !valid_profile_avatar(url) {
        return None;
    }
    cache::read(&cache::root()?, url, SystemTime::now())
}

pub(crate) type AvatarUpdates = mpsc::Receiver<Arc<Image>>;

/// Called by the profile worker after authenticated identity resolution.
pub(crate) fn profile_avatar(url: &str) -> (Option<Arc<Image>>, Option<AvatarUpdates>) {
    profile_avatar_with(cache::root(), url, SystemTime::now(), |url| {
        download(&avatar_agent(), &url)
    })
}

fn profile_avatar_with(
    root: Option<std::path::PathBuf>,
    url: &str,
    now: SystemTime,
    fetch: impl FnOnce(String) -> Option<Arc<Image>> + Send + 'static,
) -> (Option<Arc<Image>>, Option<AvatarUpdates>) {
    if !valid_profile_avatar(url) {
        return (None, None);
    }
    let cached = root.as_deref().and_then(|path| cache::read(path, url, now));
    let fresh = cached.as_ref().is_some_and(|(_, fresh)| *fresh);
    let image = cached.map(|(image, _)| image);
    if fresh {
        return (image, None);
    }
    let (tx, rx) = mpsc::sync_channel(1);
    let url = url.to_owned();
    let _ = thread::Builder::new()
        .name("herdr-profile-avatar".into())
        .spawn(move || {
            if let Some(image) = fetch(url) {
                let _ = tx.send(image);
            }
        });
    (image, Some(rx))
}

fn avatar_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .max_redirects(0)
        .build()
        .into()
}

fn valid_profile_avatar(url: &str) -> bool {
    url.len() <= 2048
        && url
            .strip_prefix("https://avatars.githubusercontent.com/")
            .is_some_and(|path| {
                !path.is_empty()
                    && path
                        .bytes()
                        .all(|b| b.is_ascii_graphic() && !matches!(b, b'\\' | b'#'))
            })
}

fn download(agent: &ureq::Agent, url: &str) -> Option<Arc<Image>> {
    if !valid_profile_avatar(url) {
        return None;
    }
    let mut response = agent.get(url).call().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let content_type = response.headers().get("content-type")?.to_str().ok()?;
    let format = avatar_format(content_type)?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(cache::LIMIT as u64)
        .read_to_vec()
        .ok()?;
    let image = cache::decode(&bytes)?;
    if image.format != format {
        return None;
    }
    if let Some(root) = cache::root() {
        let _ = cache::write(&root, url, &bytes, SystemTime::now());
    }
    Some(image)
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

    // Exercises the POSIX disk cache; other platforms have no cache root.
    #[cfg(unix)]
    #[test]
    fn profile_disk_hits_skip_network_and_stale_images_survive_failed_refresh() {
        let fixture = cache::tests::Fixture::new();
        let url = "https://avatars.githubusercontent.com/u/123?v=4";
        let now = SystemTime::now();
        let bytes = cache::tests::png();
        cache::write(&fixture.0, url, &bytes, now).expect("cache write");
        for _ in 0..2 {
            let (image, updates) = profile_avatar_with(Some(fixture.0.clone()), url, now, |_| {
                panic!("fresh disk hit must not fetch")
            });
            assert_eq!(image.expect("disk hit").bytes, bytes);
            assert!(updates.is_none());
        }
        let stale = now + Duration::from_secs(86400);
        let (release, wait) = mpsc::sync_channel(1);
        let (image, updates) =
            profile_avatar_with(Some(fixture.0.clone()), url, stale, move |_| {
                wait.recv_timeout(Duration::from_secs(5))
                    .expect("release refresh");
                None
            });
        assert_eq!(
            image.expect("stale image immediately available").bytes,
            bytes
        );
        release.send(()).expect("refresh waiting");
        assert!(matches!(
            updates
                .expect("refresh receiver")
                .recv_timeout(Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(cache::read(&fixture.0, url, stale).is_some());
        let replacement = Arc::new(Image::empty());
        let fetched = replacement.clone();
        let (image, updates) =
            profile_avatar_with(Some(fixture.0.clone()), url, stale, move |_| Some(fetched));
        assert!(image.is_some());
        assert!(Arc::ptr_eq(
            &updates
                .expect("refresh")
                .recv_timeout(Duration::from_secs(5))
                .expect("replacement"),
            &replacement
        ));
        let (image, updates) = profile_avatar_with(
            Some(fixture.0.clone()),
            "https://evil.test/avatar",
            now,
            |_| panic!("invalid URL must not fetch"),
        );
        assert!(image.is_none() && updates.is_none());
    }

    #[test]
    fn profile_avatar_urls_never_accept_credentials_or_other_hosts() {
        assert!(valid_profile_avatar(
            "https://avatars.githubusercontent.com/u/123?v=4"
        ));
        for url in [
            "http://avatars.githubusercontent.com/u/1",
            "https://avatars.githubusercontent.com.evil.test/u/1",
            "https://avatars.githubusercontent.com@evil.test/u/1",
            "https://evil.test/avatar",
            "https://user:token@avatars.githubusercontent.com/u/1",
            "https://avatars.githubusercontent.com:443/u/1",
            "https://avatars.githubusercontent.com/\\evil.test/u/1",
            "https://avatars.githubusercontent.com/u/1\n",
        ] {
            assert!(!valid_profile_avatar(url), "{url}");
        }
    }

    #[test]
    #[ignore = "requires HERDR_AVATAR_TEST_REPO and network access to GitHub avatars"]
    fn loads_local_repository_owner_avatar() {
        let cwd =
            std::env::var("HERDR_AVATAR_TEST_REPO").expect("set a local GitHub repository path");
        let owner = repo_owner(&cwd).expect("repository has a GitHub origin");
        let mut avatars = Avatars::new();
        avatars.request(&cwd);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while avatars.image(&cwd).is_none() {
            avatars.poll();
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
        // No account-name grammar is enforced: whatever GitHub names an owner,
        // an enterprise managed user included, survives the round trip.
        for owner in ["a_b", "a--b", "owner_shortcode", "-owner", "owner-", "a.b"] {
            assert_eq!(
                github_owner(&format!("https://github.com/{owner}/repo")).as_deref(),
                Some(owner),
                "{owner}"
            );
        }
        // Only what would not stay inside its own path segment is refused.
        for owner in ["", ".", "..", "a@b", "a%2fb", "a b", "a?b", "a#b", "\u{e9}"] {
            assert_eq!(
                github_owner(&format!("https://github.com/{owner}/repo")),
                None,
                "{owner}"
            );
        }
        assert!(github_owner(&format!("https://github.com/{}/repo", "a".repeat(100))).is_some());
        assert!(github_owner(&format!("https://github.com/{}/repo", "a".repeat(101))).is_none());
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
        let (requests, incoming) = mpsc::sync_channel(MAX_REQUESTS);
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
