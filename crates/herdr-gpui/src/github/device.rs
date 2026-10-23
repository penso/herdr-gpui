//! GitHub's device authorization flow: the codes it hands back, the replies it
//! can give, and the verified profile a completed flow yields. Secrets are
//! deserialized straight into redacted types and never pass through a String.

use super::{
    Error, Result,
    http::{agent, authorization, response},
    log,
    token::Credential,
    valid_token,
};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) const VERIFY_URL: &str = "https://github.com/login/device";
pub(super) const SETUP_MESSAGE: &str = "Connect with Herdr GPUI's GitHub App. Optionally set [github] oauth_client_id in config-gpui.local.toml, or HERDR_GITHUB_OAUTH_CLIENT_ID, to another GitHub App or OAuth App public client ID with Device Flow enabled. Reload GUI config after file edits.";

#[derive(Debug, Deserialize)]
pub(super) struct Device {
    pub(super) device_code: SecretString,
    pub(super) user_code: SecretString,
    pub(super) verification_uri: String,
    pub(super) expires_in: u64,
    #[serde(default = "default_interval")]
    pub(super) interval: u64,
}
pub(super) fn default_interval() -> u64 {
    5
}

impl Device {
    /// The single check that rejected this response, for diagnostics only.
    pub(super) fn rejection(&self) -> Option<&'static str> {
        let user_code = self.user_code.expose_secret();
        if !valid_token(self.device_code.expose_secret()) {
            Some("device_code")
        } else if user_code.is_empty()
            || user_code.len() > 32
            || !user_code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            Some("user_code")
        } else if self.verification_uri != VERIFY_URL {
            Some("verification_uri")
        } else if !(1..=900).contains(&self.expires_in) {
            Some("expires_in")
        } else if !(1..=900).contains(&self.interval) {
            Some("interval")
        } else {
            None
        }
    }

    pub(super) fn validate(self) -> Result<Self> {
        if let Some(rejection) = self.rejection() {
            // The verification URI is a public endpoint. An enterprise or data
            // residency host is exactly what this rejection would be hiding, so
            // it is logged; the device and user codes never are.
            tracing::warn!(
                category = "github_device",
                rejection,
                verification_uri = self.verification_uri.as_str(),
                expires_in = self.expires_in,
                interval = self.interval,
                "GitHub device authorization response rejected"
            );
            return Err(Error::Device.into());
        }
        Ok(self)
    }
}

pub(super) enum Reply {
    Device(Device, String, Instant),
    Pending(bool),
    Token(Credential),
    SignedOut,
    Authenticated(Arc<SecretString>),
    /// The GitHub CLI's account, after the user chose it and it was verified.
    GitHubCli(Profile),
    /// The account of a token the user pasted, verified and saved.
    Pasted(Profile),
}

pub(crate) struct Profile {
    pub login: String,
    pub avatar: Option<Arc<gpui::Image>>,
    pub token: Arc<SecretString>,
    pub(super) avatar_updates: Option<crate::avatars::AvatarUpdates>,
    pub source: Source,
    /// The organizations the account belongs to, read only for an additional
    /// account: the owners it covers besides its own login. `None` until
    /// GitHub listed them, so a failed listing is not mistaken for none.
    pub organizations: Option<Vec<String>>,
    /// What a GitHub App sign-in still needs installed or approved.
    pub access: super::installation::Access,
}

#[cfg(test)]
impl Profile {
    /// A signed-in account without an avatar, for tests outside this module.
    pub(crate) fn fixture(login: &str, token: Arc<SecretString>, organizations: &[&str]) -> Self {
        Self {
            login: login.into(),
            avatar: None,
            token,
            avatar_updates: None,
            source: Source::Token,
            organizations: Some(
                organizations
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            ),
            access: Default::default(),
        }
    }
}

/// Where a signed-in account's token comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Source {
    /// The environment, or a sign-in saved by this app.
    #[default]
    Token,
    /// The GitHub CLI's own sign-in, read again on every load. `repo_scope`
    /// is false when its classic scopes leave out private repositories.
    GitHubCli { repo_scope: bool },
}

impl Source {
    pub(crate) fn github_cli(self) -> bool {
        matches!(self, Self::GitHubCli { .. })
    }
}

pub(super) fn profile(token: Arc<SecretString>) -> Result<Profile> {
    profile_from(token, false)
}

/// The account `token` belongs to. `github_cli` records the GitHub CLI as
/// its source, with whether its scopes reach private repositories.
pub(super) fn profile_from(token: Arc<SecretString>, github_cli: bool) -> Result<Profile> {
    #[derive(Deserialize)]
    struct User {
        login: String,
        avatar_url: String,
    }
    let reply = agent(Duration::from_secs(15))
        .get("https://api.github.com/user")
        .header("User-Agent", "Herdr-GPUI")
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", authorization(&token)?)
        .call()
        .map_err(log::network("profile"))?;
    // Only classic OAuth tokens, which `gh auth login` issues, list scopes;
    // a token without the header is not judged by it.
    let source = if github_cli {
        Source::GitHubCli {
            repo_scope: repo_scope(
                reply
                    .headers()
                    .get("x-oauth-scopes")
                    .and_then(|value| value.to_str().ok()),
            ),
        }
    } else {
        Source::Token
    };
    let user: User = response("profile", reply)?;
    // The login is shown in the menu and logged, never persisted or placed in a
    // URL, so GitHub stays the authority on the shape of its own account names.
    // Enterprise managed users (`handle_shortcode`) are why guessing that shape
    // here was worse than useless.
    tracing::info!(
        category = "github_profile",
        login = user.login.as_str(),
        "GitHub profile loaded"
    );
    // The image transport receives no Authorization header and follows no redirects.
    let (avatar, avatar_updates) = crate::avatars::profile_avatar(&user.avatar_url);
    Ok(Profile {
        login: user.login,
        avatar,
        token,
        avatar_updates,
        source,
        organizations: None,
        access: Default::default(),
    })
}

impl Profile {
    /// Keeps the organizations `old` listed when this check could not list
    /// them, so a failed request never moves the account's repositories to
    /// the main account. Only for the same login: another account's are not
    /// this one's.
    pub(super) fn keep_organizations(&mut self, old: &Self) {
        if self.organizations.is_none() && self.login == old.login {
            self.organizations = old.organizations.clone();
        }
    }

    /// Keeps the access `old` found when this check could not tell, so a
    /// failed request neither raises nor clears the attention dot. Only for
    /// an App token: any other is never checked, so its `Unknown` is final
    /// and an App's old warning does not apply to it.
    pub(super) fn keep_access(&mut self, old: &Self) {
        if self.access == super::installation::Access::Unknown
            && self.login == old.login
            && super::installation::app_token(&self.token)
        {
            self.access = old.access.clone();
        }
    }
}

/// `profile` with what its GitHub App installations still need. A failed
/// check is only logged: the account works for public repositories either way.
pub(super) fn with_access(mut profile: Profile) -> Profile {
    match super::installation::check(&profile.token) {
        Ok(access) => profile.access = access,
        Err(error) => log::failure("installations", &error),
    }
    profile
}

const ORGANIZATIONS: &str = "query { viewer { organizations(first: 100) { nodes { login } } } }";

/// `profile` with the organizations its account belongs to, as far as its
/// token may list them. A token that may not (a classic one without
/// `read:org`, say) still covers its own login, so the failure is only logged
/// and the organizations stay unknown: [`Profile::keep_organizations`] then
/// keeps those an earlier check listed.
pub(super) fn with_organizations(mut profile: Profile) -> Profile {
    let mut cooldown = None;
    match super::graphql(
        "organizations",
        &profile.token,
        ORGANIZATIONS,
        serde_json::json!({}),
        Duration::from_secs(15),
        || false,
        &mut cooldown,
    ) {
        Ok(value) => profile.organizations = Some(organizations(&value)),
        Err(error) => log::failure("organizations", &error),
    }
    profile
}

/// The organization logins of a viewer query, bounded and checked like the
/// owners a config lists.
pub(super) fn organizations(value: &serde_json::Value) -> Vec<String> {
    value["data"]["viewer"]["organizations"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["login"].as_str())
        .filter(|login| crate::config::valid_owner(login))
        .take(100)
        .map(str::to_owned)
        .collect()
}

/// Whether a classic token's `X-OAuth-Scopes` reach private repositories.
pub(super) fn repo_scope(scopes: Option<&str>) -> bool {
    scopes.is_none_or(|scopes| scopes.split(',').any(|scope| scope.trim() == "repo"))
}

#[derive(Debug, Deserialize)]
pub(super) struct TokenResponse {
    pub(super) access_token: Option<SecretString>,
    pub(super) refresh_token: Option<SecretString>,
    expires_in: Option<u64>,
    pub(super) token_type: Option<String>,
    pub(super) error: Option<String>,
    /// Logged, never displayed: GitHub's own wording is the only place an
    /// enterprise policy or app-approval denial is explained.
    pub(super) error_description: Option<String>,
}

pub(super) fn token_reply(value: TokenResponse, client: &str) -> Result<Reply> {
    match value.error.as_deref() {
        Some("authorization_pending") => Ok(Reply::Pending(false)),
        Some("slow_down") => Ok(Reply::Pending(true)),
        Some(code) => {
            tracing::warn!(
                category = "github_oauth",
                code,
                detail = value.error_description.as_deref().unwrap_or_default(),
                "GitHub rejected the device authorization"
            );
            Err(match code {
                "expired_token" => Error::Expired,
                "access_denied" => Error::Denied,
                _ => Error::Authorization,
            }
            .into())
        }
        None => {
            let token = value
                .access_token
                .filter(|t| valid_token(t.expose_secret()))
                .ok_or_else(|| {
                    tracing::warn!(
                        category = "github_oauth",
                        "GitHub returned no usable access token"
                    );
                    Error::Token
                })?;
            if value
                .token_type
                .as_deref()
                .is_none_or(|t| !t.eq_ignore_ascii_case("bearer"))
            {
                tracing::warn!(
                    category = "github_oauth",
                    token_type = value.token_type.as_deref().unwrap_or_default(),
                    "GitHub returned an unsupported token type"
                );
                return Err(Error::TokenType.into());
            }
            Ok(Reply::Token(
                Credential::new(token, value.refresh_token, client)?
                    .with_expiry(value.expires_in, std::time::SystemTime::now()),
            ))
        }
    }
}
