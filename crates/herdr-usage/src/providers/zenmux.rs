//! ZenMux usage, read from the Management API with a Management API key
//! from the config or `ZENMUX_MANAGEMENT_API_KEY`: the rolling five-hour and
//! seven-day flow quotas, and the pay-as-you-go balance. Standard inference
//! keys are refused by these endpoints. ZenMux has no local sign-in to find,
//! and CodexBar reads no cookies for it either.

use crate::text::Text;
use crate::{
    Error, Result,
    {
        model::{Account, Balance, Kind, Provider, Report, SESSION, Section, Unit, WEEK, Window},
        probe::{Probe, Request},
        service::{Meta, Service, Setting, Timestamp, json},
        values::{invalid, plain},
    },
};
use serde::Deserialize;

const BASE: &str = "https://zenmux.ai/api/v1/management";

pub(crate) struct Zenmux;

static META: Meta = Meta::new("zenmux", "ZenMux")
    .dashboard("https://zenmux.ai/platform/management")
    .settings(&[Setting::new(
        "api_key",
        &["ZENMUX_MANAGEMENT_API_KEY"],
        "usage-zenmux-setting-api-key",
    )]);

impl Service for Zenmux {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let key = probe.setting("api_key")?;
        let subscription =
            match probe.body(Request::get(format!("{BASE}/subscription/detail")).bearer(&key)) {
                Ok(body) => body,
                Err(error) => return Some(Err(error)),
            };
        // The balance only enriches the quota, but a rejected key still fails
        // the whole refresh as CodexBar does.
        let balance = match probe.body(Request::get(format!("{BASE}/payg/balance")).bearer(&key)) {
            Ok(body) => Some(body),
            Err(Error::Rejected) => return Some(Err(Error::Rejected)),
            Err(_) => None,
        };
        Some(parse(&subscription, balance.as_deref()))
    }
}

pub(crate) fn parse(subscription: &str, balance: Option<&str>) -> Result<Report> {
    let detail = json::<Envelope<Subscription>>(subscription)?
        .into_data()
        .ok_or_else(invalid)?;
    let quotas = [
        (
            Kind::Session,
            Text::message("usage-zenmux-session"),
            detail.quota_5_hour,
            SESSION,
        ),
        (
            Kind::Weekly,
            Text::message("usage-zenmux-weekly"),
            detail.quota_7_day,
            WEEK,
        ),
    ];
    let mut flows = Vec::new();
    let mut windows = Vec::new();
    for (kind, title, quota, length) in quotas {
        let Some(quota) = quota else { continue };
        let Some(percentage) = quota.usage_percentage else {
            continue;
        };
        if let (Some(used), Some(max)) = (quota.used_flows, quota.max_flows) {
            flows.push((
                title,
                Text::message("usage-zenmux-flows")
                    .arg("used", plain(used))
                    .arg("max", plain(max))
                    .arg("flows", max),
            ));
        }
        windows.push(Window::new(
            kind,
            percentage * 100.,
            quota.resets_at.as_ref().and_then(Timestamp::time),
            Some(length),
        ));
    }
    let tier = detail
        .plan
        .as_ref()
        .and_then(|plan| plan.tier.as_deref())
        .map(capitalize)
        .filter(|tier| !tier.is_empty());
    let status = detail
        .account_status
        .as_deref()
        .map(str::trim)
        .filter(|status| !status.is_empty() && !status.eq_ignore_ascii_case("healthy"))
        .map(capitalize);
    // The tier and status are ZenMux's own words, capitalized.
    let plan = match (tier, status) {
        (Some(tier), Some(status)) => Some(
            Text::message("usage-zenmux-tier-plan-status")
                .arg("tier", tier)
                .arg("status", status),
        ),
        (Some(tier), None) => Some(Text::message("usage-zenmux-tier-plan").arg("tier", tier)),
        (None, status) => status.map(Text::service),
    };
    if let Some(expires) = detail
        .plan
        .as_ref()
        .and_then(|plan| plan.expires_at.as_ref())
        .and_then(Timestamp::time)
    {
        flows.push((
            Text::message("usage-zenmux-plan-expires"),
            Text::service(
                chrono::DateTime::<chrono::Utc>::from(expires)
                    .format("%Y-%m-%d")
                    .to_string(),
            ),
        ));
    }
    let balances = balance
        .and_then(|body| json::<Envelope<PaygBalance>>(body).ok())
        .and_then(Envelope::into_data)
        .filter(|balance| {
            balance
                .currency
                .as_deref()
                .is_some_and(|currency| currency.trim().eq_ignore_ascii_case("usd"))
        })
        .and_then(|balance| balance.total_credits)
        .filter(|credits| credits.is_finite())
        .map(|credits| {
            Balance::new(
                Text::message("usage-zenmux-payg-balance"),
                credits,
                Unit::Currency("USD".into()),
            )
        });
    Ok(
        Report::new(Provider(&Zenmux), Account { email: None, plan }, windows)
            .with_balances(balances)
            .with_sections((!flows.is_empty()).then(|| Section::Facts {
                title: Text::message("usage-zenmux-quota"),
                facts: flows,
            })),
    )
}

fn capitalize(value: &str) -> String {
    value
        .trim()
        .split(' ')
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| {
                    first
                        .to_uppercase()
                        .chain(chars.flat_map(char::to_lowercase))
                        .collect::<String>()
                })
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[derive(Deserialize)]
struct Envelope<T> {
    success: Option<bool>,
    data: Option<T>,
}

impl<T> Envelope<T> {
    fn into_data(self) -> Option<T> {
        self.success.filter(|success| *success)?;
        self.data
    }
}

#[derive(Deserialize)]
struct Subscription {
    plan: Option<Plan>,
    account_status: Option<String>,
    quota_5_hour: Option<Quota>,
    quota_7_day: Option<Quota>,
}

#[derive(Deserialize)]
struct Plan {
    tier: Option<String>,
    expires_at: Option<Timestamp>,
}

#[derive(Deserialize)]
struct Quota {
    usage_percentage: Option<f64>,
    resets_at: Option<Timestamp>,
    max_flows: Option<f64>,
    used_flows: Option<f64>,
}

#[derive(Deserialize)]
struct PaygBalance {
    currency: Option<String>,
    total_credits: Option<f64>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const SUBSCRIPTION: &str = r#"{
      "success": true,
      "data": {
        "plan": {"tier": "ultra", "amount_usd": 200, "interval": "month",
                 "expires_at": "2026-04-12T08:26:56.000Z"},
        "currency": "usd",
        "account_status": "healthy",
        "quota_5_hour": {"usage_percentage": 0.0715, "resets_at": "2026-03-24T08:35:09.000Z",
                         "max_flows": 800, "used_flows": 57.2, "remaining_flows": 742.8},
        "quota_7_day": {"usage_percentage": 0.0673, "max_flows": 6182,
                        "used_flows": 416.11, "remaining_flows": 5765.89},
        "quota_monthly": {"max_flows": 34560, "max_value_usd": 1134.33}
      }
    }"#;

    const BALANCE: &str = r#"{"success": true, "data": {"currency": "usd",
        "total_credits": 482.74, "top_up_credits": 35, "bonus_credits": 447.74}}"#;

    #[test]
    fn parses_quotas_plan_and_balance() {
        let report = parse(SUBSCRIPTION, Some(BALANCE)).unwrap();
        assert_eq!(
            report.account.plan,
            Some(Text::message("usage-zenmux-tier-plan").arg("tier", "Ultra"))
        );
        assert_eq!(report.windows.len(), 2);
        assert_eq!(report.windows[0].kind, Kind::Session);
        assert!((report.windows[0].used - 7.15).abs() < 0.001);
        assert!(report.windows[0].resets_at.is_some());
        assert_eq!(report.windows[1].kind, Kind::Weekly);
        assert!((report.windows[1].used - 6.73).abs() < 0.001);
        assert!(report.windows[1].resets_at.is_none());
        assert_eq!(report.balances.len(), 1);
        assert!((report.balances[0].amount - 482.74).abs() < 1e-9);
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        let flows = |used: &str, max: &str, flows: f64| {
            Text::message("usage-zenmux-flows")
                .arg("used", used)
                .arg("max", max)
                .arg("flows", flows)
        };
        assert_eq!(
            facts,
            &[
                (
                    Text::message("usage-zenmux-session"),
                    flows("57.20", "800", 800.)
                ),
                (
                    Text::message("usage-zenmux-weekly"),
                    flows("416.11", "6182", 6182.)
                ),
                (
                    Text::message("usage-zenmux-plan-expires"),
                    Text::service("2026-04-12")
                ),
            ]
        );
    }

    #[test]
    fn shows_unhealthy_status_and_negative_balance() {
        let body = SUBSCRIPTION.replace("\"healthy\"", "\"OVERDUE\"");
        let balance = r#"{"success":true,"data":{"currency":"usd","total_credits":-12.34}}"#;
        let report = parse(&body, Some(balance)).unwrap();
        assert_eq!(
            report.account.plan,
            Some(
                Text::message("usage-zenmux-tier-plan-status")
                    .arg("tier", "Ultra")
                    .arg("status", "Overdue")
            )
        );
        assert!((report.balances[0].amount + 12.34).abs() < 1e-9);
    }

    #[test]
    fn rejects_unsuccessful_envelope() {
        assert!(parse(r#"{"success": false}"#, None).is_err());
    }
}
