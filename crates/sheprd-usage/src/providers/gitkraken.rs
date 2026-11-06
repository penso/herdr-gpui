//! GitKraken AI usage, read from `api.gitkraken.dev` with an account session
//! access token from the config or `GITKRAKEN_API_TOKEN`: weekly personal
//! credits and, for an organization, the shared pool. Like CodexBar, this
//! finds no local sign-in: the `gk` CLI fallback and CLI session discovery
//! are not supported there either, and there is no OAuth refresh, so an
//! expired token has to be replaced by hand.

use crate::text::Text;
use crate::{
    Result,
    {
        model::{Account, Kind, Provider, Report, Section, WEEK, Window},
        probe::{Probe, Request},
        service::{Meta, Service, Setting, Timestamp, json},
        values::{count, invalid},
    },
};
use serde::Deserialize;

const URL: &str = "https://api.gitkraken.dev/v1/ai-tasks/usage";

pub(crate) struct Gitkraken;

static META: Meta = Meta::new("gitkraken", "GitKraken AI")
    .dashboard("https://gitkraken.dev/account#ai-usage")
    .settings(&[
        Setting::new(
            "token",
            &["GITKRAKEN_API_TOKEN"],
            "usage-gitkraken-setting-token",
        ),
        Setting::new(
            "org_id",
            &["GITKRAKEN_ORG_ID"],
            "usage-gitkraken-setting-org-id",
        ),
    ]);

impl Service for Gitkraken {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let token = probe.setting("token")?;
        let mut request = Request::get(URL)
            .bearer(&token)
            .header("Client-Name", "herdr-gpui")
            .header("Client-Version", env!("CARGO_PKG_VERSION"))
            .header("User-Agent", "herdr-gpui");
        if let Some(organization) = probe
            .text_setting("org_id")
            .filter(|id| !id.is_empty() && !id.contains(char::is_whitespace))
        {
            request = request.header("gk-org-id", organization);
        }
        Some(probe.body(request).and_then(|body| parse(&body)))
    }
}

pub(crate) fn parse(body: &str) -> Result<Report> {
    let usage: Usage = json(body)?;
    if usage.error.is_some() {
        return Err(invalid());
    }
    let data = usage.data.ok_or_else(invalid)?;
    let personal = Quota::new(data.used, data.limit).ok_or_else(invalid)?;
    let resets_at = data
        .resets_on
        .and_then(|reset| Timestamp::Text(reset).time())
        .ok_or_else(invalid)?;
    // Organization data is optional: when malformed it is left out and the
    // personal quota still shows.
    let pool = data
        .organization
        .and_then(|organization| Quota::new(organization.used, organization.limit));
    let mut facts = vec![(
        Text::message("usage-gitkraken-personal"),
        personal.describe(),
    )];
    let mut windows = Vec::new();
    if let Some(used) = personal.percent() {
        windows.push(Window::new(Kind::Weekly, used, Some(resets_at), Some(WEEK)));
    }
    if let Some(pool) = &pool {
        facts.push((
            Text::message("usage-gitkraken-shared-pool"),
            pool.describe(),
        ));
        if let Some(shared) = data
            .shared_used
            .filter(|shared| shared.is_finite() && *shared >= 0. && *shared <= pool.used)
        {
            facts.push((Text::message("usage-gitkraken-your-share"), credits(shared)));
            facts.push((
                Text::message("usage-gitkraken-rest"),
                credits(pool.used - shared),
            ));
        }
        if let Some(used) = pool.percent() {
            windows.push(Window::new(
                Kind::Named(Text::message("usage-gitkraken-shared-pool")),
                used,
                Some(resets_at),
                Some(WEEK),
            ));
        }
    }
    Ok(
        Report::new(Provider(&Gitkraken), Account::default(), windows).with_sections([
            Section::Facts {
                title: Text::message("usage-gitkraken-weekly-usage"),
                facts,
            },
        ]),
    )
}

fn credits(value: f64) -> Text {
    Text::message("usage-gitkraken-credits")
        .arg("amount", count(value))
        .arg("count", value)
}

/// A limit of zero is no allowance and -1 unlimited; neither has a percent.
struct Quota {
    used: f64,
    limit: f64,
}

impl Quota {
    fn new(used: Option<f64>, limit: Option<f64>) -> Option<Self> {
        let (used, limit) = (used?, limit?);
        let valid =
            used.is_finite() && used >= 0. && limit.is_finite() && (limit >= 0. || limit == -1.);
        valid.then_some(Self { used, limit })
    }

    fn percent(&self) -> Option<f64> {
        (self.limit > 0.).then(|| self.used / self.limit * 100.)
    }

    fn describe(&self) -> Text {
        let used = count(self.used);
        if self.limit > 0. {
            Text::message("usage-gitkraken-used-of")
                .arg("used", used)
                .arg("limit", count(self.limit))
                .arg("count", self.limit)
        } else if self.limit == -1. {
            Text::message("usage-gitkraken-used-unlimited")
                .arg("used", used)
                .arg("count", self.used)
        } else {
            Text::message("usage-gitkraken-used-no-allowance")
                .arg("used", used)
                .arg("count", self.used)
        }
    }
}

#[derive(Deserialize)]
struct Usage {
    data: Option<Data>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    used: Option<f64>,
    limit: Option<f64>,
    resets_on: Option<String>,
    organization: Option<Pool>,
    shared_used: Option<f64>,
}

#[derive(Deserialize)]
struct Pool {
    used: Option<f64>,
    limit: Option<f64>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn personal_and_pool_become_weekly_windows() {
        let body = r#"{"data":{"used":120,"limit":400,"resetsOn":"2026-10-05T00:00:00.000Z",
            "organization":{"used":900,"limit":3000},"sharedUsed":300},"error":null}"#;
        let report = parse(body).unwrap();
        assert_eq!(report.windows.len(), 2);
        assert_eq!(report.windows[0].kind, Kind::Weekly);
        assert!((report.windows[0].used - 30.).abs() < 0.01);
        assert_eq!(report.windows[0].length, Some(WEEK));
        assert!(report.windows[0].resets_at.is_some());
        assert_eq!(
            report.windows[1].kind,
            Kind::Named(Text::message("usage-gitkraken-shared-pool"))
        );
        assert!((report.windows[1].used - 30.).abs() < 0.01);
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        let labels: Vec<_> = facts.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "usage-gitkraken-personal",
                "usage-gitkraken-shared-pool",
                "usage-gitkraken-your-share",
                "usage-gitkraken-rest"
            ]
        );
        assert_eq!(
            facts[0].1,
            Text::message("usage-gitkraken-used-of")
                .arg("used", "120")
                .arg("limit", "400")
                .arg("count", 400)
        );
        assert_eq!(
            facts[3].1,
            Text::message("usage-gitkraken-credits")
                .arg("amount", "600")
                .arg("count", 600)
        );
    }

    #[test]
    fn unlimited_has_no_window() {
        let body = r#"{"data":{"used":55.5,"limit":-1,"resetsOn":"2026-10-05T00:00:00Z"}}"#;
        let report = parse(body).unwrap();
        assert!(report.windows.is_empty());
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        assert_eq!(
            facts[0].1,
            Text::message("usage-gitkraken-used-unlimited")
                .arg("used", "55.50")
                .arg("count", 55.5)
        );
    }

    #[test]
    fn malformed_personal_usage_fails() {
        assert!(
            parse(r#"{"data":{"used":1,"limit":-2,"resetsOn":"2026-10-05T00:00:00Z"}}"#).is_err()
        );
        assert!(parse(r#"{"data":{"used":1,"limit":2,"resetsOn":"soon"}}"#).is_err());
        assert!(parse(r#"{"data":null,"error":{"message":"x"}}"#).is_err());
    }
}
