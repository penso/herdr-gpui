//! OpenCode Go subscription usage. Sign-in sources, in CodexBar's order for
//! an unscoped account: an OpenCode API key from the config or
//! `OPENCODE_API_KEY` reads the public `zen/go/v1/usage` API; otherwise an
//! opencode.ai session cookie (the `cookie` setting, or, when OpenCode Go is
//! listed in `[usage] show_providers`, `auth` / `__Host-console_session` from
//! Chrome or Safari) reads the console's Go meters and prepaid Zen balance,
//! falling back to the legacy `/workspace/<id>/go` page for workspaces that
//! have not moved to the console.
//!
//! Not ported: the device-local cost history CodexBar reads from OpenCode's
//! SQLite database (`~/.local/share/opencode/opencode.db`), and the legacy
//! Zen balance scraped from the old dashboard HTML.

use super::opencode::{
    BASE, COOKIES, DOMAINS, USER_AGENT, call, json_windows, meter, usage, value_number,
    windows as meter_windows, workspace_id,
};
use crate::{
    Error, Result,
    usage::{
        model::{Account, Balance, Provider, Report, Section, Unit, Window},
        probe::{Probe, Request, Secret},
        service::{Meta, Service, Setting, json},
        values::invalid,
    },
};
use serde_json::{Map, Value};
use std::time::SystemTime;

const API: &str = "https://opencode.ai/zen/go/v1/usage";
const ORGS: &str = "https://opencode.ai/console/api/orgs";
const GO_STATUS: &str = "https://opencode.ai/console/api/go/status";
const BILLING_STATUS: &str = "https://opencode.ai/console/api/billing/status";
/// The console answers HTTP 400 without the workspace in this header.
const ORG_HEADER: &str = "x-org-id";
const MICRO_CENTS: f64 = 100_000_000.;

pub(crate) struct Opencodego;

static META: Meta = Meta::new("opencodego", "OpenCode Go")
    .dashboard("https://opencode.ai/auth")
    .settings(&[
        Setting::new(
            "api_key",
            &["OPENCODE_API_KEY"],
            "An OpenCode API key for the account with the Go subscription, created at \
             https://opencode.ai/auth under API Keys. Preferred over the cookie.",
        ),
        Setting::new(
            "cookie",
            &[],
            "Your opencode.ai session, used when no API key is set. Sign in at \
             https://opencode.ai/auth, open Developer Tools > Application > Cookies > \
             https://opencode.ai, and copy the __Host-console_session and auth cookies \
             (either one alone also works). Paste them as \
             \"__Host-console_session=value; auth=value\".",
        ),
        Setting::new(
            "workspace_id",
            &["CODEXBAR_OPENCODE_WORKSPACE_ID"],
            "Optional, for the cookie sign-in. The workspace to read, as a wrk_... or \
             org_... ID or its https://opencode.ai/console/... URL. Defaults to the first \
             workspace of the account.",
        ),
    ]);

impl Service for Opencodego {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        if let Some(key) = probe.setting("api_key") {
            let request = Request::get(API)
                .bearer(&key)
                .header("Accept", "application/json")
                .header("User-Agent", "CodexBar");
            return Some(
                probe
                    .body(request)
                    .and_then(|body| parse_api(&body, SystemTime::now())),
            );
        }
        let cookie = probe.cookies(DOMAINS, COOKIES)?;
        Some(fetch_web(probe, &cookie))
    }
}

fn fetch_web(probe: &mut Probe, cookie: &Secret) -> Result<Report> {
    let workspace = match probe
        .text_setting("workspace_id")
        .and_then(|raw| workspace_id(&raw))
    {
        Some(workspace) => workspace,
        None => {
            let orgs = console(probe, cookie, ORGS, None)?;
            console_workspaces(&orgs)
                .into_iter()
                .next()
                .ok_or_else(invalid)?
        }
    };
    let now = SystemTime::now();
    let status = console(probe, cookie, GO_STATUS, Some(&workspace));
    // A failed balance read does not discard valid Go usage.
    let balance = console(probe, cookie, BILLING_STATUS, Some(&workspace))
        .ok()
        .and_then(|body| parse_balance(&body));
    match status {
        Ok(body) => parse_console(&body, balance, now),
        Err(Error::UsageRejected) => Err(Error::UsageRejected),
        Err(error) if workspace.starts_with("wrk_") => {
            let page = Request::get(format!("{BASE}/workspace/{workspace}/go"))
                .cookie(cookie)
                .header("User-Agent", USER_AGENT)
                .header(
                    "Accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                );
            match call(probe, page).map(|text| usage(&text, now, true)) {
                Ok(Some(windows)) => Ok(report(windows, balance)),
                Ok(None) | Err(_) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

/// Console JSON reports a signed-out session as HTTP 401, never by page text.
fn console(
    probe: &mut Probe,
    cookie: &Secret,
    url: &str,
    workspace: Option<&str>,
) -> Result<String> {
    let mut request = Request::get(url)
        .cookie(cookie)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json");
    if let Some(workspace) = workspace {
        request = request.header(ORG_HEADER, workspace);
    }
    probe.body(request)
}

fn console_workspaces(text: &str) -> Vec<String> {
    let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| row.get("id")?.as_str())
        .filter(|id| {
            (id.starts_with("wrk_") || id.starts_with("org_"))
                && id.len() > 4
                && id[4..]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .map(str::to_owned)
        .collect()
}

fn report(windows: Vec<Window>, balance: Option<f64>) -> Report {
    Report::new(
        Provider(&Opencodego),
        Account {
            email: None,
            plan: (!windows.is_empty()).then(|| "Go".to_owned()),
        },
        windows,
    )
    .with_balances(
        balance.map(|balance| Balance::new("Zen balance", balance, Unit::Currency("USD".into()))),
    )
}

/// The public usage API: `usage.rolling/weekly/monthly`, in percent units.
pub(crate) fn parse_api(body: &str, now: SystemTime) -> Result<Report> {
    let value: Value = json(body)?;
    let usage = value
        .get("usage")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let pick = |key: &str| {
        usage
            .get(key)
            .and_then(Value::as_object)
            .and_then(|window| meter(window, now, false))
    };
    let rolling = pick("rolling").ok_or_else(invalid)?;
    Ok(report(
        meter_windows(Some(rolling), pick("weekly"), pick("monthly")),
        None,
    ))
}

/// Console Go meters in micro-cents; the month meter resets when the billing
/// period (`access.endsAt`) does. A `null` status or access is no
/// subscription, which still shows a prepaid balance.
pub(crate) fn parse_console(body: &str, balance: Option<f64>, now: SystemTime) -> Result<Report> {
    let value: Value = json(body)?;
    let Some(access) = value.get("access").and_then(Value::as_object) else {
        if value.is_null() || value.get("access").is_some_and(Value::is_null) {
            return Ok(report(Vec::new(), balance).with_sections([Section::Facts {
                title: "Subscription".into(),
                facts: vec![("OpenCode Go".into(), "None".into())],
            }]));
        }
        return json_windows(&value, now, true)
            .map(|windows| report(windows, balance))
            .ok_or_else(invalid);
    };
    let meters = access
        .get("meters")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let pick = |key: &str, fallback_reset: Option<&Value>| {
        let mut window: Map<String, Value> = meters.get(key)?.as_object()?.clone();
        if window.get("resetsAt").is_none_or(Value::is_null)
            && let Some(ends) = fallback_reset
        {
            window.insert("resetsAt".into(), ends.clone());
        }
        let used = window.get("usedMicroCents").and_then(value_number)?;
        let limit = window
            .get("limitMicroCents")
            .and_then(value_number)
            .filter(|limit| *limit > 0.)?;
        let (_, reset) = meter(&window, now, false)?;
        Some((used / limit * 100., reset))
    };
    let rolling = pick("fiveHour", None).ok_or_else(invalid)?;
    Ok(report(
        meter_windows(
            Some(rolling),
            pick("week", None),
            pick("month", access.get("endsAt")),
        ),
        balance,
    ))
}

/// The prepaid pay-as-you-go balance in USD; other billing modes have none.
pub(crate) fn parse_balance(body: &str) -> Option<f64> {
    let value: Value = serde_json::from_str(body).ok()?;
    let prepaid = value.get("billingMode").and_then(Value::as_str) == Some("prepaid")
        && value.get("mode").and_then(Value::as_str) == Some("pay-as-you-go");
    if !prepaid {
        return None;
    }
    let raw = value.get("balanceMicroCents")?.as_str()?;
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse::<f64>().ok().map(|balance| balance / MICRO_CENTS)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::usage::model::{Kind, MONTH, SESSION};
    use std::time::Duration;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    /// 2026-09-20T00:00:00Z.
    const NOW: u64 = 1_789_862_400;

    #[test]
    fn parses_api_usage_in_percent_units() {
        let report = parse_api(
            r#"{"usage":{"rolling":{"percent":0.5,"resetInSec":18100},
                "weekly":{"percent":1,"resetInSec":266500},
                "monthly":{"percent":0,"resetInSec":1539100}}}"#,
            at(NOW),
        )
        .unwrap();
        assert_eq!(report.windows.len(), 3);
        assert_eq!(report.windows[0].kind, Kind::Session);
        assert_eq!(report.windows[0].used, 0.5);
        assert_eq!(report.windows[0].resets_at, Some(at(NOW + 18_100)));
        assert_eq!(report.windows[0].length, Some(SESSION));
        assert_eq!(report.windows[1].used, 1.);
        assert_eq!(report.windows[2].kind, Kind::Monthly);
        assert_eq!(report.windows[2].length, Some(MONTH));
    }

    #[test]
    fn parses_console_meters() {
        let body = r#"{"renewalCurrency":"usd","useBalance":false,"cancelAtPeriodEnd":false,
            "access":{"startsAt":"2026-09-19T00:00:00.000Z","endsAt":"2026-10-19T00:00:00.000Z","meters":{
            "fiveHour":{"startsAt":"2026-09-19T23:00:00.000Z","resetsAt":"2026-09-20T03:00:00.000Z",
              "limitMicroCents":"1200000000","usedMicroCents":"300000000"},
            "week":{"startsAt":"2026-09-14T00:00:00.000Z","resetsAt":"2026-09-21T00:00:00.000Z",
              "limitMicroCents":"3000000000","usedMicroCents":"1200000000"},
            "month":{"limitMicroCents":"6000000000","usedMicroCents":"600000000"}}}}"#;
        let report = parse_console(body, Some(12.5), at(NOW)).unwrap();
        assert_eq!(report.windows[0].used, 25.);
        assert_eq!(report.windows[0].resets_at, Some(at(NOW + 3 * 3600)));
        assert_eq!(report.windows[1].used, 40.);
        assert_eq!(report.windows[1].resets_at, Some(at(NOW + 86_400)));
        assert_eq!(report.windows[2].used, 10.);
        assert_eq!(report.windows[2].resets_at, Some(at(NOW + 29 * 86_400)));
        assert_eq!(report.balances[0].amount, 12.5);
    }

    #[test]
    fn missing_subscription_keeps_the_balance() {
        let report = parse_console("null", Some(5.), at(NOW)).unwrap();
        assert!(report.windows.is_empty());
        assert_eq!(report.balances[0].label, "Zen balance");
        let report = parse_console(r#"{"access":null}"#, None, at(NOW)).unwrap();
        assert!(report.balances.is_empty());
    }

    #[test]
    fn reads_prepaid_balance_only() {
        let body = r#"{"billingMode":"prepaid","mode":"pay-as-you-go","balanceMicroCents":"1234567890",
            "availableMicroCents":"9876543210","creditLimitMicroCents":null}"#;
        assert!((parse_balance(body).unwrap() - 12.345_678_9).abs() < 1e-9);
        assert_eq!(
            parse_balance(r#"{"billingMode":"seat","mode":"invoiceable","balanceMicroCents":"1"}"#),
            None
        );
        assert_eq!(
            parse_balance(
                r#"{"billingMode":"prepaid","mode":"pay-as-you-go","balanceMicroCents":"1e9"}"#
            ),
            None
        );
    }

    #[test]
    fn filters_console_workspaces() {
        assert_eq!(
            console_workspaces(
                r#"[{"id":"wrk_TEST123","name":"Default"},{"id":"acc_X"},{"id":"org_Y"}]"#
            ),
            ["wrk_TEST123", "org_Y"]
        );
        assert!(console_workspaces(r#"{"error":"nope"}"#).is_empty());
    }
}
