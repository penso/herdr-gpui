//! Dependabot alerts as GitHub's GraphQL API answers them, read into the
//! shapes the section and the CLI show. Remote text is untrusted: every
//! string is cleaned and length-bounded, and links are built from validated
//! names rather than taken from the response.

use super::{Error, Report, Reports};
use crate::github::Tokens;
use chrono::{DateTime, Utc};
use herdr_client::ConnectTarget;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

/// How bad an advisory is, most severe first, so sorting puts it on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum Severity {
    Critical,
    High,
    Moderate,
    Low,
}

impl Severity {
    /// The word the CLI prints and its JSON carries.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Moderate => "moderate",
            Self::Low => "low",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::High => "High",
            Self::Moderate => "Moderate",
            Self::Low => "Low",
        }
    }

    /// Whether the bar counts it: a low or moderate alert alone is not worth
    /// a badge that never clears.
    pub(crate) fn urgent(self) -> bool {
        matches!(self, Self::Critical | Self::High)
    }
}

/// One open Dependabot alert.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Alert {
    pub(crate) number: u64,
    pub(crate) severity: Severity,
    pub(crate) package: String,
    pub(crate) ecosystem: String,
    /// The versions the advisory covers, as GitHub writes the range.
    pub(crate) vulnerable: String,
    /// The first version without it, when one is published.
    pub(crate) patched: Option<String>,
    /// The manifest or lockfile that pulls the package in.
    pub(crate) manifest: String,
    /// The advisory's GHSA identifier.
    pub(crate) advisory: String,
    pub(crate) summary: String,
    pub(crate) created_at: Option<DateTime<Utc>>,
}

impl Alert {
    /// The advisory's public page, when its identifier has GHSA's shape.
    pub(crate) fn advisory_url(&self) -> Option<String> {
        ghsa(&self.advisory).then(|| format!("https://github.com/advisories/{}", self.advisory))
    }
}

/// What a repository's alerts listing said.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Alerts {
    /// Dependabot alerts are turned off for the repository.
    Off,
    /// This sign-in may not read them. GraphQL answers such a token with an
    /// empty listing, so an empty one is checked over REST, which refuses:
    /// a GitHub App without the Dependabot alerts permission is the usual
    /// case.
    Hidden,
    /// The open alerts read, most severe first, and how many GitHub counts,
    /// which is more when the listing was capped.
    Open { alerts: Vec<Alert>, total: u64 },
}

/// One repository the window's workspaces hold, and its alerts.
#[derive(Clone, Debug)]
pub(crate) struct Repo {
    /// `owner/name` on github.com, checked by [`valid_name`].
    pub(crate) name: String,
    /// The host it was found on, as the sidebar names it.
    pub(crate) host: String,
    pub(crate) alerts: Result<Alerts, Arc<Error>>,
    pub(crate) reports: Result<Reports, Arc<Error>>,
    /// The account the listing was read with, which an item's page is read
    /// with too.
    pub(crate) account: Option<Account>,
    /// Where the window's workspaces hold it, for an agent's workspace and
    /// a manifest's file.
    pub(crate) checkouts: Vec<Checkout>,
}

/// A GitHub account's tokens, kept beside what they read. Its `Debug`
/// names no token.
#[derive(Clone)]
pub(crate) struct Account(pub(crate) Tokens);

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Account(..)")
    }
}

/// One workspace holding the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Checkout {
    pub(crate) target: ConnectTarget,
    pub(crate) workspace_id: String,
    /// The repository's root on this machine, found while reading; `None`
    /// on another host, where the code view cannot open a file.
    pub(crate) root: Option<PathBuf>,
}

impl Repo {
    /// The open alerts read, none when they are off or could not be read.
    pub(crate) fn open(&self) -> &[Alert] {
        match &self.alerts {
            Ok(Alerts::Open { alerts, .. }) => alerts,
            _ => &[],
        }
    }

    /// The private vulnerability reports in triage this account can see.
    pub(crate) fn reported(&self) -> &[Report] {
        match &self.reports {
            Ok(Reports::Open { reports, .. }) => reports,
            _ => &[],
        }
    }

    /// Whether GitHub holds more reports in triage than were read.
    pub(crate) fn more_reports(&self) -> bool {
        matches!(self.reports, Ok(Reports::Open { more: true, .. }))
    }

    /// The report's page in the repository's Security tab.
    pub(crate) fn report_url(&self, report: &Report) -> Option<String> {
        super::reports::report_url(&self.name, report)
    }

    /// The alert's page in the repository's Security tab.
    pub(crate) fn alert_url(&self, alert: &Alert) -> String {
        format!(
            "https://github.com/{}/security/dependabot/{}",
            self.name, alert.number
        )
    }

    /// The repository's Dependabot alerts page.
    pub(crate) fn alerts_url(&self) -> String {
        format!("https://github.com/{}/security/dependabot", self.name)
    }
}

/// An `owner/name` safe to put in a github.com address: two segments of the
/// characters GitHub allows in them.
pub(crate) fn valid_name(name: &str) -> bool {
    let segment = |part: &str| {
        !part.is_empty()
            && part.len() <= 100
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && part != "."
            && part != ".."
    };
    name.split_once('/')
        .is_some_and(|(owner, repo)| segment(owner) && segment(repo))
}

/// `GHSA-xxxx-xxxx-xxxx`, in the alphabet GitHub uses for the groups.
pub(super) fn ghsa(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("GHSA-") else {
        return false;
    };
    let groups: Vec<&str> = rest.split('-').collect();
    groups.len() == 3
        && groups
            .iter()
            .all(|group| group.len() == 4 && group.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// A `repository` answer of [`super::fetch::QUERY`].
pub(super) fn parse(response: &Value) -> Result<Alerts, Error> {
    let repository = response
        .get("data")
        .and_then(|data| data.get("repository"))
        .filter(|repository| !repository.is_null())
        .ok_or(Error::Output)?;
    let node = RepositoryNode::deserialize(repository)?;
    if !node.has_vulnerability_alerts_enabled {
        return Ok(Alerts::Off);
    }
    let Some(listing) = node.vulnerability_alerts else {
        return Ok(Alerts::Open {
            alerts: Vec::new(),
            total: 0,
        });
    };
    let mut alerts: Vec<Alert> = listing
        .nodes
        .into_iter()
        .flatten()
        .filter_map(AlertNode::into_alert)
        .collect();
    alerts.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    let total = listing.total_count.max(alerts.len() as u64);
    Ok(Alerts::Open { alerts, total })
}

/// Bounded, without control or bidirectional-override characters.
fn clean(text: &str) -> String {
    crate::pull_request::clean(text.trim())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryNode {
    has_vulnerability_alerts_enabled: bool,
    vulnerability_alerts: Option<Listing>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listing {
    total_count: u64,
    nodes: Vec<Option<AlertNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlertNode {
    number: u64,
    created_at: Option<DateTime<Utc>>,
    vulnerable_manifest_path: Option<String>,
    security_vulnerability: Option<VulnerabilityNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VulnerabilityNode {
    severity: Severity,
    vulnerable_version_range: Option<String>,
    first_patched_version: Option<Version>,
    package: Package,
    advisory: Advisory,
}

#[derive(Deserialize)]
struct Version {
    identifier: String,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    ecosystem: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Advisory {
    ghsa_id: String,
    summary: String,
}

impl AlertNode {
    /// An alert without its vulnerability, which GitHub sends for one it
    /// can no longer describe, is left out: there is nothing to show.
    fn into_alert(self) -> Option<Alert> {
        let vulnerability = self.security_vulnerability?;
        Some(Alert {
            number: self.number,
            severity: vulnerability.severity,
            package: clean(&vulnerability.package.name),
            ecosystem: clean(&vulnerability.package.ecosystem.to_ascii_lowercase()),
            vulnerable: vulnerability
                .vulnerable_version_range
                .as_deref()
                .map(clean)
                .unwrap_or_default(),
            patched: vulnerability
                .first_patched_version
                .map(|version| clean(&version.identifier))
                .filter(|version| !version.is_empty()),
            manifest: self
                .vulnerable_manifest_path
                .as_deref()
                .map(clean)
                .unwrap_or_default(),
            advisory: clean(&vulnerability.advisory.ghsa_id),
            summary: clean(&vulnerability.advisory.summary),
            created_at: self.created_at,
        })
    }
}
