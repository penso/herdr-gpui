//! `security`: the private vulnerability reports and Dependabot alerts the
//! Security section last read. It starts no read of its own; a window that
//! has not read yet says so. Reports carry their titles and metadata only.

use crate::{
    HerdrWindow,
    control::protocol::{
        ErrorCode, MAX_MESSAGE, Response,
        hosts::{Reply, SecurityAlertInfo, SecurityRepoInfo, SecurityReportInfo},
    },
    security::{Alert, Alerts, Item, Repo, Report, Reports},
    sidebar::ActivityPanel,
};
use gpui::{Context, Window};

/// The most alerts one answer carries, so it stays well inside the control
/// socket's message bound; each repository's `total` still counts them all.
pub(super) const MAX_ALERTS: usize = 120;
/// The most reports one answer carries.
pub(super) const MAX_REPORTS: usize = 50;
/// Room for the repositories in one answer, short of the socket's message
/// bound by the envelope and the note.
pub(super) const BUDGET: usize = MAX_MESSAGE - 4096;

pub(super) fn alerts(view: &HerdrWindow) -> Response {
    let security = &view.security;
    let (repositories, cut) = listing(security.repos());
    let note = if cut {
        Some("Showing only some of the alerts and reports; GitHub lists the rest".to_owned())
    } else if security.read_at().is_none() {
        Some(if view.main_tokens().is_none() {
            "Sign in to GitHub in Herdr UI to read Dependabot alerts".to_owned()
        } else {
            "Herdr UI has not read the alerts yet".to_owned()
        })
    } else if repositories.is_empty() {
        Some("No github.com repository is open in this window's workspaces".to_owned())
    } else {
        None
    };
    Response::Host {
        reply: Reply::Security { repositories, note },
    }
}

/// `security open`: shows an alert's or report's page in the section, as
/// the section last read it. A name the listing does not hold is refused
/// rather than read: the page belongs to the listing's account.
pub(super) fn open(
    view: &mut HerdrWindow,
    repository: &str,
    item: &str,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> Response {
    let Some(named) = Item::named(repository, item) else {
        return Response::error(
            ErrorCode::InvalidRequest,
            "Name a repository as owner/name, and an alert by its number or a report by its GHSA ID",
        );
    };
    // GitHub's names ignore case; the listing keeps the remote's spelling.
    let listed = view
        .security
        .repos()
        .iter()
        .find(|repo| repo.name.eq_ignore_ascii_case(repository))
        .map(|repo| repo.name.clone());
    let item = match (listed, named) {
        (Some(repo), Item::Alert { number, .. }) => Item::Alert { repo, number },
        (Some(repo), Item::Report { advisory, .. }) => Item::Report { repo, advisory },
        (None, _) => {
            return Response::error(
                ErrorCode::NotFound,
                format!("The Security section lists no repository {repository}"),
            );
        }
    };
    if view.security.find(&item).is_none() {
        let what = match &item {
            Item::Alert { number, .. } => format!("open alert #{number}"),
            Item::Report { advisory, .. } => format!("report {advisory} in triage"),
        };
        return Response::error(
            ErrorCode::NotFound,
            format!("The Security section lists no {what} in {}", item.repo()),
        );
    }
    let message = match &item {
        Item::Alert { repo, number } => format!("Showing alert #{number} of {repo}"),
        Item::Report { repo, advisory } => format!("Showing report {advisory} of {repo}"),
    };
    view.show_activity_panel(ActivityPanel::Security, window, cx);
    view.open_security_item(item, cx);
    Response::done(message)
}

/// Each repository with its reports and alerts, up to [`MAX_REPORTS`] and
/// [`MAX_ALERTS`] in all and within [`BUDGET`] bytes of JSON, and whether
/// any were left out here or by GitHub's own page.
pub(super) fn listing(repos: &[Repo]) -> (Vec<SecurityRepoInfo>, bool) {
    within(repos, BUDGET)
}

/// [`listing`], within `budget` bytes. The counts alone cannot bound the
/// answer: each field is capped in characters, which are up to four bytes,
/// and JSON escapes some of those to six.
pub(super) fn within(repos: &[Repo], budget: usize) -> (Vec<SecurityRepoInfo>, bool) {
    let mut alerts_left = MAX_ALERTS;
    let mut reports_left = MAX_REPORTS;
    let mut used = 0usize;
    let mut cut = false;
    let mut repositories = Vec::new();
    // Each item is charged its JSON and the comma before it.
    let mut fits = |size: usize| {
        let after = used.saturating_add(size).saturating_add(1);
        if after > budget {
            return false;
        }
        used = after;
        true
    };
    for repo in repos {
        let mut info = shell(repo);
        if !fits(json_len(&info)) {
            cut = true;
            break;
        }
        cut |= info.more_reports;
        for report in repo.reported() {
            let item = report_info(repo, report);
            if reports_left == 0 || !fits(json_len(&item)) {
                cut = true;
                break;
            }
            reports_left -= 1;
            info.reports.push(item);
        }
        for alert in repo.open() {
            let item = alert_info(repo, alert);
            if alerts_left == 0 || !fits(json_len(&item)) {
                cut = true;
                break;
            }
            alerts_left -= 1;
            info.alerts.push(item);
        }
        repositories.push(info);
    }
    (repositories, cut)
}

fn json_len(value: &impl serde::Serialize) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

/// The repository's states, without its alerts and reports yet.
fn shell(repo: &Repo) -> SecurityRepoInfo {
    let (state, total, error) = match &repo.alerts {
        Ok(Alerts::Open { total, .. }) => ("open", *total, None),
        Ok(Alerts::Off) => ("off", 0, None),
        Ok(Alerts::Hidden) => ("hidden", 0, None),
        Err(error) => ("error", 0, Some(error.to_string())),
    };
    let (reporting, reports_error) = match &repo.reports {
        Ok(Reports::Open { .. }) => ("on", None),
        Ok(Reports::Off) => ("off", None),
        Ok(Reports::Hidden) => ("hidden", None),
        Err(error) => ("error", Some(error.to_string())),
    };
    SecurityRepoInfo {
        name: repo.name.clone(),
        host: repo.host.clone(),
        url: repo.alerts_url(),
        state: state.to_owned(),
        total,
        alerts: Vec::new(),
        error,
        reporting: reporting.to_owned(),
        reports: Vec::new(),
        more_reports: repo.more_reports(),
        reports_error,
    }
}

fn alert_info(repo: &Repo, alert: &Alert) -> SecurityAlertInfo {
    SecurityAlertInfo {
        number: alert.number,
        severity: alert.severity.name().to_owned(),
        package: alert.package.clone(),
        ecosystem: alert.ecosystem.clone(),
        summary: alert.summary.clone(),
        advisory: alert.advisory.clone(),
        advisory_url: alert.advisory_url(),
        vulnerable: alert.vulnerable.clone(),
        patched: alert.patched.clone(),
        manifest: alert.manifest.clone(),
        url: repo.alert_url(alert),
    }
}

fn report_info(repo: &Repo, report: &Report) -> SecurityReportInfo {
    SecurityReportInfo {
        advisory: report.advisory.clone(),
        summary: report.summary.clone(),
        severity: report.severity.map(|severity| severity.name().to_owned()),
        reporter: report.reporter.clone(),
        created_at: report.created_at.map(|at| at.to_rfc3339()),
        url: repo.report_url(report),
    }
}
