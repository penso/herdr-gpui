//! A GitLab project's open issues, read through GitLab's REST API in
//! agent-launcher's GitLab row format: `native_id` is the issue's project
//! `iid`. Every sync is a complete listing, so it may replace the cache.
//!
//! The token comes from the `GITLAB_TOKEN` (or `GITLAB_ACCESS_TOKEN`) a local
//! checkout's `.envrc` sets (`envrc`), else from the login shell's for the
//! instance `GITLAB_HOST` names, gitlab.com by default, else from the one
//! pasted in the account menu (`saved`), else from the GitLab CLI's own
//! sign-in there (`account`). Without one, public projects still list.
//!
//! A remote on an SSH host of its own, such as `gitlab-ssh.example.com` for
//! an instance served at `gitlab.example.com`, is read from the instance
//! whose `ssh_host` the GitLab CLI's config names it, as glab itself does.

pub mod account;
pub mod envrc;
pub mod oauth;
pub mod saved;

use super::{Activity, Checkpoint, Item, ItemKey, Provider, SourceKey};
use chrono::{DateTime, SecondsFormat, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{
    collections::HashSet,
    ffi::OsString,
    io::Read,
    time::{Duration, Instant, SystemTime},
};
use zeroize::Zeroizing;

pub mod cli_hosts;

const PAGE: usize = 100;
/// The most open issues one sync reads. A project with more is shown but
/// not cached, since a capped listing is no full sync.
pub const MAX_OPEN: usize = 2_000;
const TIMEOUT: Duration = Duration::from_secs(30);
/// One page of 100 issues, descriptions included.
const LIMIT: u64 = 4 * 1024 * 1024;
const CLI_TIMEOUT: Duration = Duration::from_secs(10);
/// A token is at most a few hundred bytes; the rest is room for warnings.
const CLI_OUTPUT_LIMIT: usize = 16 * 1024;
/// Where the GitLab CLI's environment tokens apply when `GITLAB_HOST` is unset.
const DEFAULT_HOST: &str = "gitlab.com";
/// The variables the GitLab CLI reads a token from, in its order.
const TOKEN_VARIABLES: [&str; 2] = ["GITLAB_TOKEN", "GITLAB_ACCESS_TOKEN"];

/// Why GitLab could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("GitLab could not be reached at {origin}: {source}")]
    Network {
        /// The scheme and host tried, so a wrong one shows.
        origin: String,
        #[source]
        source: Box<ureq::Error>,
    },
    #[error("GitLab's answer could not be read: {0}")]
    Read(#[source] std::io::Error),
    #[error("GitLab's answer exceeded the size limit")]
    Size,
    #[error("GitLab's answer is not the expected JSON: {0}")]
    Json(#[source] serde_json::Error),
    #[error("GitLab's address for {0:?} is not valid")]
    Address(String),
    #[error("The GitLab token is not one an HTTP header can carry")]
    Token,
    #[error(
        "GitLab shows {repository} only when signed in: add a token from the account button at the top right"
    )]
    SignIn { repository: String },
    #[error(
        "GitLab shows {repository} only when signed in, and a token is never sent over plain HTTP: use an https:// remote"
    )]
    InsecureRemote { repository: String },
    #[error("GitLab refused the token; replace it from the account button at the top right")]
    Authentication,
    #[error(
        "GitLab refused GITLAB_TOKEN; change or unset it, since it is tried before a token saved in the app"
    )]
    EnvironmentRefused,
    #[error(
        "GitLab refused the GITLAB_TOKEN this checkout's .envrc sets; change it there, since it is tried first"
    )]
    EnvrcRefused,
    #[error("GitLab refused the token's access; it needs the read_api scope")]
    Scope,
    #[error("GitLab refused access to {repository}; the token needs the read_api scope")]
    Forbidden { repository: String },
    #[error("GitLab shows no project {repository} to this token")]
    NotFound { repository: String },
    #[error("GitLab rate limit reached; it is read again later")]
    RateLimit,
    #[error("GitLab request failed (HTTP {0})")]
    Status(u16),
    #[error("GitLab's sign-in was declined")]
    Denied,
    #[error("The GitLab sign-in code expired; connect again")]
    CodeExpired,
    #[error("{0:?} is not a GitLab host name")]
    Host(String),
    #[error(
        "GitLab at {0} is read over plain HTTP, and a token is only ever sent over HTTPS: use an https:// remote"
    )]
    InsecureHost(String),
    #[error("GitLab tokens are saved for at most {} hosts", saved::MAX_HOSTS)]
    TooManyHosts,
    #[error("The saved GitLab tokens are not in the expected form")]
    Record,
    #[error("The folder for saved GitLab tokens could not be found: {0}")]
    Directory(#[source] herdr_client::local_path::PathError),
    #[cfg(target_os = "linux")]
    #[error(
        "No desktop keyring is running to save a GitLab token in. Start GNOME Keyring, KWallet, or KeePassXC, set [gitlab] allow_plaintext_credentials = true, or use GITLAB_TOKEN"
    )]
    NoKeyring(#[source] herdr_credentials::Error),
    #[error(
        "No secure credential store is available to save a GitLab token in. Set [gitlab] allow_plaintext_credentials = true, or use GITLAB_TOKEN"
    )]
    NoStore(#[source] herdr_credentials::Error),
    #[error("The GitLab token could not be read or saved: {0}")]
    Store(#[source] herdr_credentials::Error),
}

impl From<herdr_credentials::Error> for Error {
    fn from(error: herdr_credentials::Error) -> Self {
        match error {
            #[cfg(target_os = "linux")]
            herdr_credentials::Error::SecretServiceUnavailable => Self::NoKeyring(error),
            herdr_credentials::Error::Policy => Self::NoStore(error),
            error => Self::Store(error),
        }
    }
}

/// The `[gitlab]` table.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Saves pasted tokens in the private `0600` `gitlab-credentials` file
    /// when no desktop keyring is running. Same meaning as `[github]`'s.
    pub allow_plaintext_credentials: bool,
    /// The OAuth application ID that connects accounts on a self-managed
    /// instance, by instance (`"gitlab.example.com" = "<id>"`). gitlab.com
    /// uses Herdr UI's own unless one is set here.
    pub applications: std::collections::BTreeMap<String, String>,
}

impl Config {
    /// The OAuth application that connects accounts on `instance`, if any.
    pub fn application(&self, instance: &str) -> Option<&str> {
        self.applications
            .iter()
            .find(|(host, _)| saved::normalize_host(host).as_deref() == Some(instance))
            .map(|(_, application)| application.as_str())
            .or_else(|| (instance == DEFAULT_HOST).then_some(oauth::GITLAB_COM_APPLICATION))
    }

    /// Where pasted tokens are saved.
    pub fn store(&self) -> herdr_credentials::Store {
        herdr_credentials::Store::for_policy(self.allow_plaintext_credentials)
    }
}

impl Error {
    /// A failure only the user can fix, such as a missing or refused token.
    pub fn needs_user(&self) -> bool {
        matches!(
            self,
            Self::SignIn { .. }
                | Self::InsecureRemote { .. }
                | Self::Authentication
                | Self::EnvironmentRefused
                | Self::EnvrcRefused
                | Self::Scope
                | Self::Forbidden { .. }
                | Self::NotFound { .. }
        )
    }

    /// A missing or refused token, which a token pasted in Settings >
    /// GitLab fixes; the view offers that page rather than a failure.
    pub fn needs_sign_in(&self) -> bool {
        matches!(self, Self::SignIn { .. } | Self::Authentication)
    }

    /// The same failure for a token from `source`: a refused `GITLAB_TOKEN`
    /// asks for the shell or `.envrc` that sets it to change, since a token
    /// saved in the app is only tried after it.
    pub fn from_source(self, source: account::Source) -> Self {
        match (self, source) {
            (Self::Authentication, account::Source::Environment) => Self::EnvironmentRefused,
            (Self::Authentication, account::Source::Envrc) => Self::EnvrcRefused,
            (error, _) => error,
        }
    }
}

/// One sync's result. Only a `complete` listing may replace the cache.
pub struct Listing {
    pub items: Vec<Item>,
    pub checkpoint: Checkpoint,
    pub complete: bool,
}

/// How the API is reached. The shared source key names only the host, as
/// agent-launcher keys it, so the scheme comes from the remote's own URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    /// HTTP for an `http://` remote, whose host may serve nothing else;
    /// HTTPS for every other remote, SSH ones included.
    pub fn of_remote(url: &str) -> Self {
        match url::Url::parse(url) {
            Ok(url) if url.scheme() == "http" => Self::Http,
            _ => Self::Https,
        }
    }

    /// How `source` is read: over its repository's remote's scheme when that
    /// remote is the source's, else HTTPS.
    pub fn of_source(info: Option<&super::repo::RepoInfo>, source: &SourceKey) -> Self {
        info.and_then(|info| info.remote.as_ref())
            .filter(|remote| remote.source == *source)
            .map_or(Self::Https, |remote| Self::of_remote(&remote.url))
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

/// `source` as a GitLab project when its host's name alone reads as GitHub
/// but the GitLab CLI knows that host, as an instance or an instance's
/// `ssh_host`. Only the provider changes; the host and path stay as
/// agent-launcher keys them. `cli` reads glab's config, only when needed.
/// Returns whether `source` changed.
pub fn recognize(source: &mut SourceKey, cli: impl FnOnce() -> Vec<cli_hosts::CliHost>) -> bool {
    if source.provider != Provider::Github || source.host == "github.com" {
        return false;
    }
    let known = cli_hosts::knows(&cli(), &source.host);
    if known {
        source.provider = Provider::Gitlab;
    }
    known
}

/// Where a project's API is. The remote's host, unless the GitLab CLI names
/// it as an instance's `ssh_host`: then that instance's, at its `api_host`
/// over its `api_protocol`. The source key keeps the remote's host either way,
/// as agent-launcher keys it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Api {
    /// The instance's name, which tokens are saved under and `GITLAB_HOST`
    /// names.
    pub instance: String,
    /// Where requests go: the instance, unless the CLI sets another.
    pub host: String,
    pub scheme: Scheme,
}

impl Api {
    /// The API of a remote on `host` reached over `scheme`, given the GitLab
    /// CLI's instances.
    pub fn new(host: &str, scheme: Scheme, cli: &[cli_hosts::CliHost]) -> Self {
        let Some(mapped) = cli_hosts::by_ssh_host(cli, host) else {
            return Self {
                instance: host.to_owned(),
                host: host.to_owned(),
                scheme,
            };
        };
        Self {
            instance: mapped.host.clone(),
            host: mapped
                .api_host
                .clone()
                .unwrap_or_else(|| mapped.host.clone()),
            scheme: Self::cli_scheme(Some(mapped)),
        }
    }

    /// The API of instance `host`, as typed in the account menu or named by
    /// a saved token: the GitLab CLI's `api_host` and `api_protocol` for it,
    /// else the instance itself over HTTPS.
    pub fn of_instance(host: &str, cli: &[cli_hosts::CliHost]) -> Self {
        let named = cli
            .iter()
            .find(|named| named.host.eq_ignore_ascii_case(host));
        Self {
            instance: host.to_owned(),
            host: named
                .and_then(|named| named.api_host.clone())
                .unwrap_or_else(|| host.to_owned()),
            scheme: Self::cli_scheme(named),
        }
    }

    /// glab reads an instance's API over HTTPS unless told otherwise.
    fn cli_scheme(named: Option<&cli_hosts::CliHost>) -> Scheme {
        match named.and_then(|named| named.api_protocol.as_deref()) {
            Some(protocol) if protocol.eq_ignore_ascii_case("http") => Scheme::Http,
            _ => Scheme::Https,
        }
    }

    /// [`Self::new`] with this machine's GitLab CLI config. Reads a file:
    /// call it from a worker.
    pub fn resolve(host: &str, scheme: Scheme) -> Self {
        Self::new(host, scheme, &cli_hosts::read())
    }

    fn origin(&self) -> String {
        format!("{}://{}", self.scheme.as_str(), self.host)
    }
}

/// What a request carries: a token only over HTTPS, since anyone who can
/// watch a plain HTTP connection could take it. Over HTTP the token is
/// withheld and the read is anonymous, which public projects allow.
#[derive(Clone, Copy, Debug)]
pub enum Credential<'a> {
    Anonymous,
    Withheld,
    Token(&'a SecretString),
}

impl<'a> Credential<'a> {
    pub fn new(scheme: Scheme, token: Option<&'a SecretString>) -> Self {
        match (scheme, token) {
            (_, None) => Self::Anonymous,
            (Scheme::Http, Some(_)) => Self::Withheld,
            (Scheme::Https, Some(token)) => Self::Token(token),
        }
    }
}

/// Lists `source`'s open issues from `api`, reading GitLab as `token` when
/// given and the scheme keeps it private. `previous` keeps
/// agent-launcher's markers, which herdr-gpui never adds.
pub fn sync(
    (source, api): (&SourceKey, &Api),
    token: Option<&SecretString>,
    previous: Option<&Checkpoint>,
    cancelled: &impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> super::Result<Listing> {
    let agent = agent(TIMEOUT);
    let credential = Credential::new(api.scheme, token);
    let mut fetch = |after| {
        if cancelled() {
            return Err(super::Error::Cancelled);
        }
        Ok(fetch_page(
            &agent,
            (source, api),
            credential,
            after,
            cooldown,
        )?)
    };
    full(source, previous.cloned().unwrap_or_default(), &mut fetch)
}

/// Reads every open issue, oldest first, each page continuing from the
/// creation time the last one reached. Numbered pages would not do: an issue
/// edited or closed while the listing is read shifts the ones after it
/// between pages, so one could be skipped and then dropped from the cache.
/// Creation times never change, so this order holds still; `created_after`
/// includes its bound, and issues read twice are counted once.
pub fn full(
    source: &SourceKey,
    previous: Checkpoint,
    fetch: &mut impl FnMut(Option<DateTime<Utc>>) -> super::Result<Vec<IssueNode>>,
) -> super::Result<Listing> {
    let started = Utc::now();
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut complete = true;
    let mut after = None;
    loop {
        let page = fetch(after)?;
        let more = page.len() >= PAGE;
        let last = page.last().map(|node| node.created_at);
        let before = items.len();
        items.extend(
            page.into_iter()
                .filter(|node| seen.insert(node.iid))
                .map(|node| node.into_item(source)),
        );
        if !more {
            break;
        }
        // A page of issues all created at one instant cannot be passed.
        if items.len() >= MAX_OPEN || items.len() == before {
            complete = false;
            break;
        }
        after = last;
    }
    Ok(Listing {
        items,
        checkpoint: Checkpoint {
            updated_at: Some(started),
            etag: None,
            last_full_at: Some(Utc::now()),
            pr_details: previous.pr_details,
            pr_cursor: previous.pr_cursor,
        },
        complete,
    })
}

/// The API address of `source`'s open issues created at or after `after`.
pub fn issues_url(
    (source, api): (&SourceKey, &Api),
    after: Option<DateTime<Utc>>,
) -> Result<url::Url, Error> {
    let invalid = || Error::Address(source.repository.clone());
    let base = format!("{}/api/v4/projects", api.origin());
    let mut url = url::Url::parse(&base).map_err(|_| invalid())?;
    // One segment: the project's full path, its slashes escaped as GitLab asks.
    url.path_segments_mut()
        .map_err(|()| invalid())?
        .push(&source.repository)
        .push("issues");
    let mut query = url.query_pairs_mut();
    query
        .append_pair("state", "opened")
        .append_pair("scope", "all")
        .append_pair("order_by", "created_at")
        .append_pair("sort", "asc")
        .append_pair("per_page", &PAGE.to_string());
    if let Some(after) = after {
        // Truncating to milliseconds moves the bound earlier, never past an issue.
        query.append_pair(
            "created_after",
            &after.to_rfc3339_opts(SecondsFormat::Millis, true),
        );
    }
    drop(query);
    Ok(url)
}

fn fetch_page(
    agent: &ureq::Agent,
    (source, api): (&SourceKey, &Api),
    credential: Credential<'_>,
    after: Option<DateTime<Utc>>,
    cooldown: &mut Option<Duration>,
) -> Result<Vec<IssueNode>, Error> {
    let mut request = agent
        .get(issues_url((source, api), after)?.as_str())
        .header("User-Agent", "Herdr-GPUI")
        .header("Accept", "application/json");
    if let Credential::Token(token) = credential {
        request = request.header("Authorization", authorization(token)?);
    }
    let mut reply = request.call().map_err(|error| Error::Network {
        origin: api.origin(),
        source: Box::new(error),
    })?;
    let status = reply.status().as_u16();
    if status != 200 {
        *cooldown = retry_after(status, reply.headers(), SystemTime::now());
        tracing::warn!(
            category = "orchestrator_gitlab",
            status,
            host = api.host.as_str(),
            "GitLab refused an issue listing"
        );
        return Err(status_error(status, &source.repository, credential));
    }
    serde_json::from_slice(&body(&mut reply, LIMIT)?).map_err(Error::Json)
}

/// An agent for GitLab's API: bounded by `timeout`, following no redirect,
/// so a token is never carried to another address, and returning refusals
/// as statuses to read.
fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

/// A reply's body, refused when it is larger than `limit`.
fn body(reply: &mut ureq::http::Response<ureq::Body>, limit: u64) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    reply
        .body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::Read)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Size);
    }
    Ok(bytes)
}

fn authorization(token: &SecretString) -> Result<ureq::http::HeaderValue, Error> {
    let text = Zeroizing::new(format!("Bearer {}", token.expose_secret()));
    let mut header = ureq::http::HeaderValue::from_str(&text).map_err(|_| Error::Token)?;
    header.set_sensitive(true);
    Ok(header)
}

/// What a refusal means. GitLab answers a private project as missing to a
/// reader without a token, so that asks for one rather than blaming the remote.
pub fn status_error(status: u16, repository: &str, credential: Credential<'_>) -> Error {
    let repository = repository.to_owned();
    match (status, credential) {
        (401 | 403 | 404, Credential::Anonymous) => Error::SignIn { repository },
        (401 | 403 | 404, Credential::Withheld) => Error::InsecureRemote { repository },
        (401, Credential::Token(_)) => Error::Authentication,
        (403, Credential::Token(_)) => Error::Forbidden { repository },
        (404, Credential::Token(_)) => Error::NotFound { repository },
        (429, _) => Error::RateLimit,
        (status, _) => Error::Status(status),
    }
}

/// How long a rate-limited project waits: `retry-after` seconds, else until
/// `ratelimit-reset`, bounded to between five minutes and a day.
pub fn retry_after(
    status: u16,
    headers: &ureq::http::HeaderMap,
    now: SystemTime,
) -> Option<Duration> {
    if status != 429 {
        return None;
    }
    let seconds = |name| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
    };
    let epoch = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let wait = seconds("retry-after")
        .or_else(|| seconds("ratelimit-reset").map(|reset| reset.saturating_sub(epoch)))
        .unwrap_or(3600);
    Some(Duration::from_secs(wait.clamp(300, 86400)))
}

/// The token to read instance `host` as and where it came from, in
/// [`account::lookup`]'s order. Blocks on the credential store and
/// subprocesses, which `cancelled` stops: call it from a worker.
pub fn token(
    host: &str,
    store: herdr_credentials::Store,
    checkout: Option<&std::path::Path>,
    cancelled: &impl Fn() -> bool,
) -> Option<account::Found> {
    account::lookup(host, store, checkout, cancelled)
}

/// The GitLab CLI's environment token, sent only to the host it names, so a
/// gitlab.com token never reaches a self-managed instance's remote.
pub fn environment_token(
    host: &str,
    variable: impl Fn(&str) -> Option<OsString>,
) -> Option<SecretString> {
    let meant_for = variable("GITLAB_HOST")
        .and_then(|value| value.into_string().ok())
        .map(|value| configured_host(&value))
        .unwrap_or_else(|| DEFAULT_HOST.to_owned());
    if !meant_for.eq_ignore_ascii_case(host) {
        return None;
    }
    TOKEN_VARIABLES
        .iter()
        .filter_map(|name| variable(name)?.into_string().ok())
        .find_map(|value| valid(value.trim()))
}

/// `GITLAB_HOST` may be a bare host or a URL, as the GitLab CLI accepts.
fn configured_host(value: &str) -> String {
    let value = value.trim();
    url::Url::parse(value)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .and_then(|url| {
            let host = url.host_str()?.to_owned();
            Some(
                url.port()
                    .map_or(host.clone(), |port| format!("{host}:{port}")),
            )
        })
        .unwrap_or_else(|| value.trim_end_matches('/').to_owned())
}

/// `glab config get token --host <host>`, under the bounded process policy
/// shared with PR lookups. Its environment tokens are removed, so the
/// answer is the CLI's saved sign-in for this host alone.
fn cli_token(host: &str) -> Option<SecretString> {
    let mut command = herdr_client::new_command("glab");
    // An app opened from a desktop launcher has a minimal PATH.
    herdr_client::login_env::apply(&mut command);
    for name in TOKEN_VARIABLES
        .into_iter()
        .chain(["GITLAB_HOST", "OAUTH_TOKEN"])
    {
        command.env_remove(name);
    }
    command.args(["config", "get", "token", "--host", host]);
    let result = herdr_github::pull_request::run_bytes(
        &mut command,
        Instant::now() + CLI_TIMEOUT,
        &|| false,
        CLI_OUTPUT_LIMIT,
    );
    match result {
        Ok((true, output)) => parse_cli(&output),
        Ok((false, _)) => None,
        Err(error) => {
            tracing::debug!(
                category = "orchestrator_gitlab",
                error = %error,
                "The GitLab CLI could not be asked for a token"
            );
            None
        }
    }
}

/// The CLI prints the token alone, or nothing when it holds none.
pub fn parse_cli(output: &[u8]) -> Option<SecretString> {
    let mut lines = std::str::from_utf8(output)
        .ok()?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    match (lines.next(), lines.next()) {
        (Some(token), None) => valid(token),
        _ => None,
    }
}

/// A token is a short run of visible ASCII; anything else would corrupt the
/// header or is not a token at all.
fn valid(token: &str) -> Option<SecretString> {
    (!token.is_empty() && token.len() <= 1024 && token.bytes().all(|byte| byte.is_ascii_graphic()))
        .then(|| token.into())
}

#[derive(Deserialize)]
struct Author {
    username: String,
}

#[derive(Deserialize)]
pub struct IssueNode {
    iid: u64,
    title: String,
    description: Option<String>,
    state: String,
    web_url: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    author: Option<Author>,
    #[serde(default)]
    labels: Vec<String>,
    user_notes_count: Option<u64>,
}

impl IssueNode {
    pub fn into_item(self, source: &SourceKey) -> Item {
        Item {
            key: ItemKey {
                source: source.clone(),
                native_id: self.iid.to_string(),
            },
            identifier: format!("#{}", self.iid),
            title: self.title,
            description: self.description.filter(|body| !body.is_empty()),
            // GitLab says `opened`; the list reads `open` as GitHub's.
            state: match self.state.as_str() {
                "opened" => "open".to_owned(),
                state => state.to_ascii_lowercase(),
            },
            url: Some(self.web_url),
            author: self.author.map(|author| author.username),
            labels: self.labels,
            parent_id: None,
            blocked_by: Vec::new(),
            priority: None,
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
            pull_request: None,
            activity: Some(Activity {
                comments: self.user_notes_count,
                ..Activity::default()
            }),
        }
    }
}
