use gpui::{Image, ImageFormat};
pub(crate) use herdr_github::github_repo;
mod cache;
mod links;
mod profiles;
use herdr_client::{ConnectTarget, protocol::ClientShellWorkspace};
pub(crate) use profiles::ProfileAvatars;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime},
};

const MAX_REQUESTS: usize = 128;

/// The whole budget for reading one remote checkout's Git configuration.
const REMOTE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a checkout whose host could not be read, while its device stayed
/// connected, waits before a sidebar update may ask again. A device that
/// reconnects is asked at once instead (see `Avatars::follow`).
const RETRY_AFTER: Duration = Duration::from_secs(60);

/// SSH reads that may run at once. A host that stops answering holds a
/// reader for up to `REMOTE_TIMEOUT`, so a few readers keep one slow host
/// from stalling every other device's avatars.
const REMOTE_READERS: usize = 4;

/// A checkout whose repository owner's avatar a sidebar row shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Checkout<S> {
    /// A workspace directory on this machine, read by local Git.
    Local(S),
    /// A directory on a saved SSH device, read there: the Git directory its
    /// daemon reported, else the workspace directory.
    Ssh { target: S, dir: S },
}

impl<'a> Checkout<&'a str> {
    /// The checkout behind `workspace` on the device reached by `target`.
    /// Only the local daemon's directories are read by this machine's Git; a
    /// saved SSH device is read over SSH. Its daemon reports a Git directory,
    /// shared by a repository's worktrees, for only some checkouts, so one
    /// with a branch but none is read by its workspace directory. Other
    /// devices may be forwarded from an unknown machine and get none.
    pub(crate) fn of(
        local: bool,
        target: &'a ConnectTarget,
        workspace: &'a ClientShellWorkspace,
    ) -> Option<Self> {
        match target {
            ConnectTarget::Ssh { target, .. } => {
                let dir = match (&workspace.worktree, &workspace.branch) {
                    (Some(tree), _) => &tree.key,
                    // Without a branch it is no Git checkout: nothing to dial for.
                    (None, Some(_)) => &workspace.new_workspace_cwd,
                    (None, None) => return None,
                };
                Some(Self::Ssh { target, dir })
            }
            _ if local => Some(Self::Local(&workspace.new_workspace_cwd)),
            _ => None,
        }
    }

    fn to_owned(self) -> Checkout<String> {
        match self {
            Self::Local(cwd) => Checkout::Local(cwd.to_owned()),
            Self::Ssh { target, dir } => Checkout::Ssh {
                target: target.to_owned(),
                dir: dir.to_owned(),
            },
        }
    }
}

impl Checkout<String> {
    fn as_deref(&self) -> Checkout<&str> {
        match self {
            Self::Local(cwd) => Checkout::Local(cwd),
            Self::Ssh { target, dir } => Checkout::Ssh { target, dir },
        }
    }
}

/// What is known about one checkout's avatar.
#[derive(Clone, Debug)]
enum Slot {
    /// Requested at `asked` and not yet answered.
    Pending { asked: Instant },
    /// The owner's avatar, or `None` for a checkout without a GitHub owner.
    Resolved(Option<Arc<Image>>),
    /// The host could not be read; asked again from `retry_at`, or as soon as
    /// its device reconnects.
    Unreachable { retry_at: Instant },
}

/// A worker's answer for one checkout: an error only when the host could
/// not be read, never when it was read and has no GitHub owner.
type Lookup = herdr_client::Result<Option<Arc<Image>>>;

type Results = mpsc::SyncSender<(Checkout<String>, Lookup)>;
type Refresh = mpsc::SyncSender<(Checkout<String>, String)>;

/// Memory front end to the shared public-image disk cache.
pub struct Avatars {
    // Local Git answers at once while an SSH read may take `REMOTE_TIMEOUT`,
    // so each has its own queue and a slow host never delays a local avatar.
    local_requests: mpsc::SyncSender<Checkout<String>>,
    remote_requests: mpsc::SyncSender<Checkout<String>>,
    results: mpsc::Receiver<(Checkout<String>, Lookup)>,
    // Presence deduplicates pending requests as well as resolved hits and misses.
    local: HashMap<String, Slot>,
    // Keyed by SSH target, then directory, so a row looks up without allocating.
    remote: HashMap<String, HashMap<String, Slot>>,
    // Each connected SSH endpoint's host and when its connection came up.
    links: HashMap<String, (String, Instant)>,
    offline: links::Offline,
}

impl Avatars {
    pub fn new() -> Self {
        let (local_requests, local_incoming) = mpsc::sync_channel(MAX_REQUESTS);
        let (remote_requests, remote_incoming) = mpsc::sync_channel(MAX_REQUESTS);
        let (outgoing, results) = mpsc::sync_channel(MAX_REQUESTS * 2);
        let (refresh, network) = mpsc::sync_channel::<(Checkout<String>, String)>(MAX_REQUESTS);
        let updated = outgoing.clone();
        thread::spawn(move || {
            let agent = avatar_agent();
            let mut owners = HashMap::new();
            for (checkout, url) in network {
                let image = owners
                    .entry(url.clone())
                    .or_insert_with(|| download(&agent, &url))
                    .clone();
                if let Some(image) = image
                    && updated.send((checkout, Ok(Some(image)))).is_err()
                {
                    break;
                }
            }
        });
        let offline = links::Offline::default();
        // Deliberately detach: dropping the senders ends the workers without a UI join.
        let remote_incoming = Arc::new(Mutex::new(remote_incoming));
        for _ in 0..REMOTE_READERS {
            let (incoming, offline, outgoing, refresh) = (
                remote_incoming.clone(),
                offline.clone(),
                outgoing.clone(),
                refresh.clone(),
            );
            thread::spawn(move || {
                // The lock is held only while waiting, never across a read.
                while let Some(checkout) = incoming.lock().ok().and_then(|rx| rx.recv().ok()) {
                    if !answer(checkout, &offline, &outgoing, &refresh) {
                        break;
                    }
                }
            });
        }
        let local_offline = offline.clone();
        thread::spawn(move || {
            for checkout in local_incoming {
                if !answer(checkout, &local_offline, &outgoing, &refresh) {
                    break;
                }
            }
        });
        Self {
            local_requests,
            remote_requests,
            results,
            local: HashMap::new(),
            remote: HashMap::new(),
            links: HashMap::new(),
            offline,
        }
    }

    /// Enqueue each exact checkout at most once, including ones without a
    /// GitHub owner. A host that could not be read is asked again once its
    /// retry time has passed at `now`.
    pub(crate) fn request(&mut self, checkout: Checkout<&str>, now: Instant) {
        match self.slot(checkout) {
            Some(Slot::Unreachable { retry_at }) if now >= *retry_at => {}
            Some(_) => return,
            None => {
                let len = self.local.len() + self.remote.values().map(HashMap::len).sum::<usize>();
                if len >= MAX_REQUESTS {
                    return;
                }
            }
        }
        let requests = match checkout {
            Checkout::Local(_) => &self.local_requests,
            Checkout::Ssh { .. } => &self.remote_requests,
        };
        if requests.try_send(checkout.to_owned()).is_ok() {
            self.insert(checkout.to_owned(), Slot::Pending { asked: now });
        }
    }

    /// Drain completed requests without blocking. Misses also count as
    /// updates; a host that could not be read is retried after `now`.
    pub fn poll(&mut self, now: Instant) -> bool {
        let mut changed = false;
        while let Ok((checkout, lookup)) = self.results.try_recv() {
            // A read that failed after its device reconnected missed that
            // connection's snapshot, which found it still pending, so it is
            // asked again here rather than waiting for another snapshot.
            let reconnected = lookup.is_err() && self.reconnected_since_asked(checkout.as_deref());
            let slot = match lookup {
                Ok(image) => Slot::Resolved(image),
                // Stopped because its device disconnected: due again as soon
                // as one is connected, since only then is it asked for.
                Err(herdr_client::Error::Ssh(herdr_client::ssh::Error::Cancelled)) => {
                    Slot::Unreachable { retry_at: now }
                }
                Err(_) if reconnected => Slot::Unreachable { retry_at: now },
                Err(_) => Slot::Unreachable {
                    retry_at: now + RETRY_AFTER,
                },
            };
            if reconnected {
                self.insert(checkout.clone(), slot);
                self.request(checkout.as_deref(), now);
            } else {
                self.insert(checkout, slot);
            }
            changed = true;
        }
        changed
    }

    pub(crate) fn image(&self, checkout: Checkout<&str>) -> Option<Arc<Image>> {
        match self.slot(checkout)? {
            Slot::Resolved(image) => image.clone(),
            Slot::Pending { .. } | Slot::Unreachable { .. } => None,
        }
    }

    fn slot(&self, checkout: Checkout<&str>) -> Option<&Slot> {
        match checkout {
            Checkout::Local(cwd) => self.local.get(cwd),
            Checkout::Ssh { target, dir } => self.remote.get(target)?.get(dir),
        }
    }

    fn insert(&mut self, checkout: Checkout<String>, image: Slot) {
        match checkout {
            Checkout::Local(cwd) => {
                self.local.insert(cwd, image);
            }
            Checkout::Ssh { target, dir } => {
                self.remote.entry(target).or_default().insert(dir, image);
            }
        }
    }
}

/// Look up one checkout's owner and send what the disk cache holds for it,
/// queueing a download when that is missing or stale. False once the UI end
/// is gone.
fn answer(
    checkout: Checkout<String>,
    offline: &links::Offline,
    outgoing: &Results,
    refresh: &Refresh,
) -> bool {
    let url = match repo_owner(checkout.as_deref(), offline) {
        Ok(owner) => {
            owner.map(|owner| format!("https://avatars.githubusercontent.com/{owner}?size=48"))
        }
        Err(error) => return outgoing.send((checkout, Err(error))).is_ok(),
    };
    let cached = url.as_deref().and_then(cached_image);
    let fresh = cached.as_ref().is_some_and(|(_, fresh)| *fresh);
    if outgoing
        .send((checkout.clone(), Ok(cached.map(|(image, _)| image))))
        .is_err()
    {
        return false;
    }
    if !fresh && let Some(url) = url {
        let _ = refresh.try_send((checkout, url));
    }
    true
}

/// The GitHub owner behind a checkout's `upstream` remote, else its `origin`.
/// Fails only when a remote host could not be read, or went offline before
/// or during the read; local Git failures and absent remotes are `None`.
fn repo_owner(
    checkout: Checkout<&str>,
    offline: &links::Offline,
) -> herdr_client::Result<Option<String>> {
    let deadline = Instant::now() + REMOTE_TIMEOUT;
    for key in ["remote.upstream.url", "remote.origin.url"] {
        let url = match checkout {
            Checkout::Local(cwd) => local_config(cwd, key),
            // The upstream read may use most of the budget; origin gets only
            // what is left.
            // A read queued before its device disconnected never dials.
            Checkout::Ssh { target, .. } if links::is_offline(offline, target) => {
                return Err(herdr_client::ssh::Error::Cancelled.into());
            }
            Checkout::Ssh { target, dir } => herdr_client::remote_config_value(
                target,
                dir,
                key,
                deadline
                    .checked_duration_since(Instant::now())
                    .ok_or(herdr_client::ssh::Error::Timeout)?,
                || links::is_offline(offline, target),
            )?,
        };
        if let Some(owner) = url.as_deref().and_then(github_owner) {
            return Ok(Some(owner));
        }
    }
    Ok(None)
}

fn local_config(cwd: &str, key: &str) -> Option<String> {
    let output = herdr_client::new_command("git")
        .args(["-C", cwd, "config", "--default", "", "--get", key])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let text = std::str::from_utf8(&output.stdout).ok()?;
    Some(text.trim_end_matches(['\r', '\n']).to_owned())
}

fn github_owner(remote: &str) -> Option<String> {
    github_repo(remote).map(|(owner, _)| owner)
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
mod tests;
