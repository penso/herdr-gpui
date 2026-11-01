//! Manus credits, read with the manus.im web session: the value of its
//! `session_id` cookie, sent as a bearer token. The value comes from the
//! `session_token` setting (`MANUS_SESSION_TOKEN`/`MANUS_SESSION_ID`), else
//! from a Cookie header in the `cookie` setting (`MANUS_COOKIE`), else from
//! Chrome or Safari when Manus is listed in `show_providers`.
//!
//! Unlike CodexBar, a rejected browser session is not retried with the next
//! one: an explicitly set token is simply tried first.

use crate::model::Count;
use crate::text::Text;
use crate::{
    Result,
    {
        model::{
            Account, Balance, Kind, Provider, Report, Section, Unit, Window, group, title_case,
        },
        probe::{Probe, Request},
        service::{Meta, Service, Setting, Timestamp, json, number},
        values::invalid,
    },
};
use serde::Deserialize;
use std::time::{Duration, SystemTime};

const URL: &str = "https://api.manus.im/user.v1.UserService/GetAvailableCredits";
const AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";
/// Numeric refresh times count from 2001-01-01, Foundation's reference date.
const REFERENCE_EPOCH: u64 = 978_307_200;

pub(crate) struct Manus;

static META: Meta = Meta::new("manus", "Manus")
    .dashboard("https://manus.im")
    .settings(&[
        Setting::new(
            "session_token",
            &["MANUS_SESSION_TOKEN", "MANUS_SESSION_ID"],
            "usage-manus-setting-session-token",
        ),
        Setting::new("cookie", &["MANUS_COOKIE"], "usage-manus-setting-cookie"),
    ]);

impl Service for Manus {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let token = probe
            .setting("session_token")
            .or_else(|| probe.cookie_value(&["manus.im"], "session_id"))?;
        let request = Request::post(URL)
            .bearer(&token)
            .header("Origin", "https://manus.im")
            .header("Referer", "https://manus.im/")
            .header("Connect-Protocol-Version", "1")
            .header("User-Agent", AGENT)
            .json("{}");
        Some(probe.body(request).and_then(|body| parse(&body)))
    }
}

/// The credits object may come bare or inside a `data`, `result`,
/// `response`, or `availableCredits` envelope.
pub(crate) fn parse(body: &str) -> Result<Report> {
    let root: Envelope = json(body)?;
    let credits = [
        root.data,
        root.result,
        root.response,
        root.available_credits,
    ]
    .into_iter()
    .flatten()
    .next()
    .unwrap_or(root.credits);
    if !credits.any() {
        return Err(invalid());
    }
    let total = credits.total_credits.unwrap_or(0.);
    let free = credits.free_credits.unwrap_or(0.);
    let monthly = credits.pro_monthly_credits.unwrap_or(0.);
    let periodic = credits.periodic_credits.unwrap_or(0.);
    let refresh = credits.refresh_credits.unwrap_or(0.);
    let max_refresh = credits.max_refresh_credits.unwrap_or(0.);
    let mut windows = Vec::new();
    if monthly > 0. {
        windows.push(Window::new(
            Kind::Named(Text::message("usage-manus-monthly-credits")),
            (monthly - periodic) / monthly * 100.,
            None,
            None,
        ));
    }
    if max_refresh > 0. {
        let reset = credits.next_refresh_time.as_ref().and_then(refresh_time);
        windows.push(Window::new(
            Kind::Named(Text::message("usage-manus-daily-refresh")),
            (max_refresh - refresh) / max_refresh * 100.,
            reset,
            None,
        ));
    }
    let count = |value: f64| group(value.round() as i64);
    let of = |left: f64, total: f64| {
        Text::message("usage-manus-left-of")
            .arg("left", count(left))
            .arg("total", count(total))
    };
    let mut facts = vec![(
        Text::message("usage-manus-free"),
        Text::service(count(free)),
    )];
    if monthly > 0. {
        facts.push((Text::message("usage-manus-monthly"), of(periodic, monthly)));
    }
    if max_refresh > 0. {
        let label = credits
            .refresh_interval
            .as_deref()
            .map(str::trim)
            .filter(|interval| !interval.is_empty())
            // The service's own interval, such as `daily`, names it.
            .map_or_else(
                || Text::message("usage-manus-refresh"),
                |interval| Text::service(title_case(interval)),
            );
        facts.push((label, of(refresh, max_refresh)));
    }
    Ok(Report::new(Provider(&Manus), Account::default(), windows)
        .with_balances([Balance::new(
            Text::message("usage-manus-balance"),
            total,
            Unit::Count(Count::Credits),
        )])
        .with_sections([Section::Facts {
            title: Text::message("usage-manus-credits"),
            facts,
        }]))
}

/// Seconds since 2001 as a number, or an RFC 3339 string.
fn refresh_time(value: &Timestamp) -> Option<SystemTime> {
    match value {
        Timestamp::Number(seconds) if seconds.is_finite() && *seconds > 0. => {
            SystemTime::UNIX_EPOCH
                .checked_add(Duration::from_secs(REFERENCE_EPOCH))?
                .checked_add(Duration::from_secs_f64(*seconds))
        }
        Timestamp::Number(_) => None,
        Timestamp::Text(text) if text.len() > 10 && text.as_bytes().get(10) == Some(&b'T') => {
            value.time()
        }
        Timestamp::Text(_) => None,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    data: Option<Credits>,
    result: Option<Credits>,
    response: Option<Credits>,
    available_credits: Option<Credits>,
    #[serde(flatten)]
    credits: Credits,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credits {
    #[serde(default, deserialize_with = "number")]
    total_credits: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    free_credits: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    periodic_credits: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    addon_credits: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    refresh_credits: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    max_refresh_credits: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pro_monthly_credits: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    event_credits: Option<f64>,
    next_refresh_time: Option<Timestamp>,
    refresh_interval: Option<String>,
}

impl Credits {
    /// Timing fields alone are not a credits answer.
    fn any(&self) -> bool {
        [
            self.total_credits,
            self.free_credits,
            self.periodic_credits,
            self.addon_credits,
            self.refresh_credits,
            self.max_refresh_credits,
            self.pro_monthly_credits,
            self.event_credits,
        ]
        .iter()
        .any(Option::is_some)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn monthly_and_daily_credits() {
        let body = r#"{
            "totalCredits": 4300,
            "freeCredits": 300,
            "periodicCredits": 3000,
            "addonCredits": 0,
            "refreshCredits": 100,
            "maxRefreshCredits": 300,
            "proMonthlyCredits": 4000,
            "eventCredits": 0,
            "nextRefreshTime": "2026-09-26T00:00:00Z",
            "refreshInterval": "daily"
        }"#;
        let report = parse(body).unwrap();
        assert_eq!(report.windows.len(), 2);
        // Named windows sort by name: "Daily refresh" before "Monthly credits".
        let monthly = &report.windows[1];
        assert_eq!(
            monthly.kind,
            Kind::Named(Text::message("usage-manus-monthly-credits"))
        );
        assert_eq!(monthly.percent(), 25);
        let daily = &report.windows[0];
        assert_eq!(daily.percent(), 67);
        assert!(daily.resets_at.is_some());
        assert_eq!(report.balances[0].amount, 4300.);
        assert_eq!(report.balances[0].unit, Unit::Count(Count::Credits));
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("facts");
        };
        assert_eq!(
            facts,
            &[
                (Text::message("usage-manus-free"), Text::service("300")),
                (
                    Text::message("usage-manus-monthly"),
                    Text::message("usage-manus-left-of")
                        .arg("left", "3,000")
                        .arg("total", "4,000")
                ),
                (
                    Text::service("Daily"),
                    Text::message("usage-manus-left-of")
                        .arg("left", "100")
                        .arg("total", "300")
                ),
            ]
        );
        assert_eq!(
            report.balances[0].label,
            Text::message("usage-manus-balance")
        );
    }

    #[test]
    fn envelope_and_reference_date() {
        let body = r#"{"data": {"totalCredits": "12", "maxRefreshCredits": 300,
            "refreshCredits": 300, "nextRefreshTime": 780000000}}"#;
        let report = parse(body).unwrap();
        assert_eq!(report.balances[0].amount, 12.);
        let daily = &report.windows[0];
        assert_eq!(daily.percent(), 0);
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("facts");
        };
        assert_eq!(facts[1].0, Text::message("usage-manus-refresh"));
        assert_eq!(
            daily.resets_at,
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(978_307_200 + 780_000_000))
        );
    }

    #[test]
    fn error_object_is_not_credits() {
        assert!(parse(r#"{"code": "unauthenticated", "message": "no"}"#).is_err());
        assert!(parse(r#"{"nextRefreshTime": 1}"#).is_err());
    }
}
