//! Claude plan usage, read with Claude Code's own sign-in: the login keychain
//! on a Mac, else `.credentials.json` in its config directory. Other accounts
//! claude-swap manages are listed beneath it. A Claude Code pointed at another
//! endpoint, or signing in through an API key helper, is not using that
//! sign-in, so it is left alone.

pub(crate) mod cswap;

use crate::text::Text;
use crate::{
    Result,
    {
        model::{Account, Kind, Provider, Report, SESSION, Section, WEEK, Window, title_case},
        probe::{HostPath, Probe, Request, Secret},
        service::{Meta, Service, Timestamp, json},
    },
};
use cswap::Accounts;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

pub(crate) const URL: &str = "https://api.anthropic.com/api/oauth/usage";
pub(crate) const BETA: &str = "oauth-2025-04-20";
/// The usage endpoint serves Claude Code's OAuth clients.
pub(crate) const AGENT: &str = "claude-code/2.1.0";
pub(crate) const KEYCHAIN: &str = "Claude Code-credentials";
/// Where Claude Code sends requests unless `ANTHROPIC_BASE_URL` says otherwise.
const API: &str = "https://api.anthropic.com";

pub(crate) struct Claude;

/// What the sign-in says about itself, which the usage response does not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SignIn {
    pub plan: Option<String>,
    pub tier: Option<String>,
    pub email: Option<String>,
}

static META: Meta = Meta::new("claude", "Claude")
    .icon("icons/agent-claude.svg")
    .dashboard("https://claude.ai/settings/usage")
    .status_page("https://status.anthropic.com");

impl Service for Claude {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let Accounts { active, others } = cswap::accounts(probe);
        let report = match signed_in(probe) {
            Some(report) => report,
            // Without Claude Code's OAuth sign-in, such as on an API-key
            // login, cswap's reading of the active account is all there is.
            None => Ok(Report::new(
                Provider(&Claude),
                Account {
                    email: Some(Text::from(active.as_ref()?.email.clone())),
                    plan: None,
                },
                active.filter(|active| !active.windows.is_empty())?.windows,
            )),
        };
        Some(report.map(|report| {
            if others.is_empty() {
                report
            } else {
                report.with_detail(others)
            }
        }))
    }
}

/// The usage of the account Claude Code is signed in with.
fn signed_in(probe: &mut Probe) -> Option<Result<Report>> {
    let directory = |rest: &str| HostPath::env_or("CLAUDE_CONFIG_DIR", ".claude", rest);
    if elsewhere(probe, &directory("settings.json")) {
        return None;
    }
    let credentials = keychain(probe).or_else(|| probe.file(&directory(".credentials.json")))?;
    let token = probe.field(&credentials, &["claudeAiOauth", "accessToken"])?;
    let email = [directory(".claude.json"), HostPath::home(".claude.json")]
        .iter()
        .find_map(|path| probe.file_text(path, &["oauthAccount", "emailAddress"]));
    let sign_in = SignIn {
        plan: probe.text(&credentials, &["claudeAiOauth", "subscriptionType"]),
        tier: probe.text(&credentials, &["claudeAiOauth", "rateLimitTier"]),
        email,
    };
    let request = Request::get(URL)
        .bearer(&token)
        .header("anthropic-beta", BETA)
        .header("User-Agent", AGENT);
    Some(probe.body(request).and_then(|body| parse(&body, sign_in)))
}

/// Whether Claude Code works without its OAuth sign-in: `ANTHROPIC_BASE_URL`,
/// from the host's environment or the `env` of its `settings`, names another
/// endpoint, or an `apiKeyHelper` supplies its key. An OAuth token still in
/// the keychain is then a leftover, possibly revoked, and asking Anthropic's
/// usage endpoint with it only repeats a request the service rejects (#708).
pub(crate) fn elsewhere(probe: &mut Probe, settings: &HostPath) -> bool {
    let base = probe
        .env_text("ANTHROPIC_BASE_URL")
        .or_else(|| probe.file_text(settings, &["env", "ANTHROPIC_BASE_URL"]));
    base.is_some_and(|url| !anthropic(&url))
        || probe.file_text(settings, &["apiKeyHelper"]).is_some()
}

/// Whether `url` is Anthropic's own API, with or without a path after it.
fn anthropic(url: &str) -> bool {
    let url = url.trim().trim_end_matches('/');
    url == API
        || url
            .strip_prefix(API)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Claude Code keeps its sign-in under the user's account name. Asked by
/// service alone, `security` may answer with another account's item first,
/// such as one `sudo claude` left under `root` holding only MCP sign-ins, so
/// the user's own item is asked for before any item of the service.
pub(crate) fn keychain(probe: &mut Probe) -> Option<Secret> {
    if !probe.is_macos() {
        return None;
    }
    // Claude Code namespaces explicit profiles by the NFC-normalized config
    // directory. Never fall back to the generic service for a selected profile:
    // that can silently report another account's usage.
    let service = match probe.env_path("CLAUDE_CONFIG_DIR") {
        Some(path) => {
            let digest = Sha256::digest(path.nfc().collect::<String>().as_bytes());
            format!(
                "{KEYCHAIN}-{:02x}{:02x}{:02x}{:02x}",
                digest[0], digest[1], digest[2], digest[3]
            )
        }
        None => KEYCHAIN.to_owned(),
    };
    let user = probe.env_text("USER");
    [user.as_deref(), None].into_iter().find_map(|account| {
        let credentials = probe.keychain(&service, account)?;
        probe.field(&credentials, &["claudeAiOauth", "accessToken"])?;
        Some(credentials)
    })
}

pub(crate) fn parse(body: &str, sign_in: SignIn) -> Result<Report> {
    let usage: Usage = json(body)?;
    let mut windows: Vec<Window> = usage
        .limits
        .unwrap_or_default()
        .into_iter()
        .filter_map(|limit| {
            let kind = match limit.kind.as_str() {
                "session" => Kind::Session,
                "weekly_all" => Kind::Weekly,
                "weekly_scoped" => Kind::Named(Text::from(
                    limit
                        .scope?
                        .model?
                        .display_name
                        .filter(|name| !name.trim().is_empty())?,
                )),
                _ => return None,
            };
            let length = if kind == Kind::Session { SESSION } else { WEEK };
            Some(Window::new(
                kind,
                limit.percent?,
                limit.resets_at.as_ref().and_then(Timestamp::time),
                Some(length),
            ))
        })
        .collect();
    if windows.is_empty() {
        windows = [
            (Kind::Session, usage.five_hour, SESSION),
            (Kind::Weekly, usage.seven_day, WEEK),
        ]
        .into_iter()
        .filter_map(|(kind, window, length)| {
            let window = window?;
            Some(Window::new(
                kind,
                window.utilization?,
                window.resets_at.as_ref().and_then(Timestamp::time),
                Some(length),
            ))
        })
        .collect();
    }
    let account = Account {
        email: sign_in.email.map(Text::from),
        plan: plan(sign_in.plan.as_deref(), sign_in.tier.as_deref()).map(Text::from),
    };
    let shares: Vec<_> = usage
        .seven_day_breakdown
        .map(|breakdown| breakdown.rows)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| Some((row.display_name?, row.percent.filter(|p| *p > 0.)? as f32)))
        .collect();
    let spend = usage.spend.map(|spend| Section::Facts {
        title: "Extra usage".into(),
        facts: if spend.enabled.unwrap_or(false) {
            let used = spend.used.as_ref().and_then(Money::text);
            let limit = spend.limit.as_ref().and_then(Money::text);
            vec![(
                "This month".into(),
                match (used, limit) {
                    (Some(used), Some(limit)) => format!("{used} of {limit}").into(),
                    (Some(used), None) => Text::from(used),
                    _ => "On".into(),
                },
            )]
        } else {
            vec![("Status".into(), "Off".into())]
        },
    });
    Ok(
        Report::new(Provider(&Claude), account, windows).with_sections(
            (!shares.is_empty())
                .then(|| Section::Shares {
                    title: "This week by surface".into(),
                    shares: crate::text::shares(shares),
                })
                .into_iter()
                .chain(spend),
        ),
    )
}

/// `max` on tier `default_claude_max_20x` is `Max 20x`.
pub(crate) fn plan(subscription: Option<&str>, tier: Option<&str>) -> Option<String> {
    let name = title_case(subscription?);
    let multiple = tier
        .and_then(|tier| tier.rsplit('_').next())
        .filter(|last| {
            last.strip_suffix('x')
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        });
    (!name.is_empty()).then(|| match multiple {
        Some(multiple) => format!("{name} {multiple}"),
        None => name,
    })
}

#[derive(Deserialize)]
struct Usage {
    limits: Option<Vec<Limit>>,
    five_hour: Option<FixedWindow>,
    seven_day: Option<FixedWindow>,
    seven_day_breakdown: Option<Breakdown>,
    spend: Option<Spend>,
}

#[derive(Deserialize)]
struct Limit {
    kind: String,
    percent: Option<f64>,
    resets_at: Option<Timestamp>,
    scope: Option<Scope>,
}

#[derive(Deserialize)]
struct Scope {
    model: Option<Model>,
}

#[derive(Deserialize)]
struct Model {
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct FixedWindow {
    utilization: Option<f64>,
    resets_at: Option<Timestamp>,
}

#[derive(Deserialize)]
struct Breakdown {
    rows: Vec<BreakdownRow>,
}

#[derive(Deserialize)]
struct BreakdownRow {
    display_name: Option<String>,
    percent: Option<f64>,
}

#[derive(Deserialize)]
struct Spend {
    enabled: Option<bool>,
    used: Option<Money>,
    limit: Option<Money>,
}

#[derive(Deserialize)]
struct Money {
    amount_minor: i64,
    currency: String,
    exponent: u32,
}

impl Money {
    /// `12.34 EUR`, in the currency's own minor units.
    fn text(&self) -> Option<String> {
        let scale = 10_i64.checked_pow(self.exponent.min(6))?;
        let sign = if self.amount_minor < 0 { "-" } else { "" };
        let amount = self.amount_minor.unsigned_abs();
        let scale = scale.unsigned_abs();
        let currency = self.currency.trim();
        Some(if self.exponent == 0 {
            format!("{sign}{amount} {currency}")
        } else {
            format!(
                "{sign}{}.{:0width$} {currency}",
                amount / scale,
                amount % scale,
                width = self.exponent.min(6) as usize
            )
        })
    }
}
