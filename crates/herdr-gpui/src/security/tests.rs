#![allow(clippy::unwrap_used)]

use super::{Alert, Alerts, Error, INTERVAL, Repo, Report, Reports, Security, Severity};
use crate::{HerdrWindow, sidebar::ActivityPanel};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

mod accounts;
mod filter;
mod page;
mod pane;
mod pane_resize;
mod parse;
mod partial;
mod reports;
mod saved;
mod section;
mod state;
mod unreadable;

/// One alert node in the shape GitHub's GraphQL API answers with.
fn node(number: u64, severity: &str, created: &str, package: &str) -> Value {
    json!({
        "number": number,
        "createdAt": created,
        "vulnerableManifestPath": "Cargo.lock",
        "securityVulnerability": {
            "severity": severity,
            "vulnerableVersionRange": ">= 0.15.0, < 0.20.0",
            "firstPatchedVersion": {"identifier": "0.20.0"},
            "package": {"name": package, "ecosystem": "RUST"},
            "advisory": {"ghsaId": "GHSA-wrw7-89jp-8q8g", "summary": "Unsound iterator"},
        },
    })
}

fn response(enabled: bool, total: u64, nodes: Vec<Value>) -> Value {
    json!({"data": {"repository": {
        "hasVulnerabilityAlertsEnabled": enabled,
        "vulnerabilityAlerts": {"totalCount": total, "nodes": nodes},
    }}})
}

fn alert(number: u64, severity: Severity) -> Alert {
    Alert {
        number,
        severity,
        package: "glib".into(),
        ecosystem: "rust".into(),
        vulnerable: "< 0.20.0".into(),
        patched: Some("0.20.0".into()),
        manifest: "Cargo.lock".into(),
        advisory: "GHSA-wrw7-89jp-8q8g".into(),
        summary: "Unsound iterator".into(),
        created_at: None,
    }
}

fn repo(name: &str, alerts: Result<Alerts, Error>) -> Repo {
    Repo {
        name: name.into(),
        host: "Local".into(),
        alerts: alerts.map_err(Arc::new),
        reports: Ok(Reports::Off),
        account: None,
        checkouts: Vec::new(),
    }
}

fn report(advisory: &str, severity: Option<Severity>, created: &str) -> Report {
    Report {
        advisory: advisory.into(),
        summary: format!("Report {advisory}"),
        severity,
        reporter: Some("finder".into()),
        created_at: Some(created.parse().unwrap()),
    }
}

fn reported(name: &str, reports: Vec<Report>) -> Repo {
    Repo {
        reports: Ok(Reports::Open {
            reports,
            more: false,
        }),
        ..repo(name, Ok(Alerts::Off))
    }
}

fn open(alerts: Vec<Alert>) -> Alerts {
    let total = alerts.len() as u64;
    Alerts::Open { alerts, total }
}
