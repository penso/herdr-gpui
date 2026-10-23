//! Private vulnerability reports: what outsiders filed through a
//! repository's Security tab, waiting in triage as draft advisories. GitHub
//! lists them only over REST, and only to the repository's admins, security
//! managers, and the advisory's collaborators; anyone else reads none.
//!
//! A report describes a vulnerability that is not fixed yet, so the listing
//! reads only its title and metadata: the description and any proof of
//! concept are skipped while parsing and never reach the list or the CLI.
//! Its page reads them when the user opens it, and keeps them in memory
//! only (`super::page`).

use super::{Error, Severity, model::ghsa};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);
/// Reports per repository: a queue this long is already a fire.
pub(super) const PAGE: usize = 50;

/// One report in triage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Report {
    /// The draft advisory's GHSA identifier.
    pub(crate) advisory: String,
    pub(crate) summary: String,
    /// What the reporter rated it, if they did.
    pub(crate) severity: Option<Severity>,
    pub(crate) reporter: Option<String>,
    pub(crate) created_at: Option<DateTime<Utc>>,
}

/// What a repository's private reporting said.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Reports {
    /// Private vulnerability reporting is turned off.
    Off,
    /// This sign-in may not read them: GitHub refused or hid the endpoint,
    /// as it does for a GitHub App without the Repository security
    /// advisories permission.
    Hidden,
    /// The reports in triage this account can see, most severe then newest
    /// first, and whether GitHub holds more than the [`PAGE`] read.
    Open { reports: Vec<Report>, more: bool },
}

/// The reports of `owner/repo`, a name [`super::model::valid_name`] checked.
pub(super) fn read(
    name: &str,
    tokens: &crate::github::Tokens,
    cancelled: impl Fn() -> bool,
) -> Result<Reports, Error> {
    let (owner, _) = name.split_once('/').ok_or(Error::Output)?;
    let token = tokens.for_owner(owner);
    let enabled = match crate::github::rest::<Reporting>(
        "security",
        token,
        &format!("/repos/{name}/private-vulnerability-reporting"),
        TIMEOUT,
        &cancelled,
    ) {
        Ok(reporting) => reporting.enabled,
        Err(error) if hidden(&error) => return Ok(Reports::Hidden),
        Err(error) => return Err(Error::Github(Box::new(error))),
    };
    if !enabled {
        return Ok(Reports::Off);
    }
    // One more than the page, so a longer queue says it was cut.
    match crate::github::rest::<Vec<AdvisoryNode>>(
        "security",
        token,
        &format!(
            "/repos/{name}/security-advisories?state=triage&per_page={}",
            PAGE + 1
        ),
        TIMEOUT,
        &cancelled,
    ) {
        Ok(nodes) => Ok(parse(nodes)),
        Err(error) if hidden(&error) => Ok(Reports::Hidden),
        Err(error) => Err(Error::Github(Box::new(error))),
    }
}

/// A refusal that means this account may not look, not that the read
/// failed: a spent quota is a different error and stays one.
pub(super) fn hidden(error: &crate::Error) -> bool {
    matches!(
        error,
        crate::Error::GitHub(crate::github::Error::Forbidden | crate::github::Error::Status(404))
    )
}

/// `{"enabled": bool}`.
#[derive(Deserialize)]
pub(super) struct Reporting {
    pub(super) enabled: bool,
}

/// A page of repository advisories in triage, of up to one more than
/// [`PAGE`]: the extra one only says the queue goes on.
pub(super) fn parse(nodes: Vec<AdvisoryNode>) -> Reports {
    let mut reports: Vec<Report> = nodes
        .into_iter()
        .filter(|node| node.state == "triage")
        .map(AdvisoryNode::into_report)
        .collect();
    // Unrated reports after rated ones: `None` would otherwise sort first.
    reports.sort_by(|a, b| {
        (a.severity.is_none(), a.severity)
            .cmp(&(b.severity.is_none(), b.severity))
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    let more = reports.len() > PAGE;
    reports.truncate(PAGE);
    Reports::Open { reports, more }
}

/// The report's page in the repository's Security tab, when its identifier
/// has GHSA's shape.
pub(super) fn report_url(repository: &str, report: &Report) -> Option<String> {
    ghsa(&report.advisory).then(|| {
        format!(
            "https://github.com/{repository}/security/advisories/{}",
            report.advisory
        )
    })
}

/// Only what the list shows: no `description`, `vulnerabilities`, or
/// `cvss`, which serde skips unread, so the report's substance is never held.
#[derive(Deserialize)]
pub(super) struct AdvisoryNode {
    ghsa_id: String,
    summary: String,
    state: String,
    severity: Option<String>,
    author: Option<Login>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

impl AdvisoryNode {
    fn into_report(self) -> Report {
        let clean = |text: &str| crate::pull_request::clean(text.trim());
        Report {
            advisory: clean(&self.ghsa_id),
            summary: clean(&self.summary),
            severity: self.severity.as_deref().and_then(rest_severity),
            reporter: self
                .author
                .map(|author| clean(&author.login))
                .filter(|login| !login.is_empty()),
            created_at: self.created_at,
        }
    }
}

/// REST's severity words, which say `medium` where GraphQL says `MODERATE`.
fn rest_severity(word: &str) -> Option<Severity> {
    match word {
        "critical" => Some(Severity::Critical),
        "high" => Some(Severity::High),
        "medium" | "moderate" => Some(Severity::Moderate),
        "low" => Some(Severity::Low),
        _ => None,
    }
}
