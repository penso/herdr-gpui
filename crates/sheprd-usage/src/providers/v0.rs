//! v0 usage, read from the v0 Platform API with an API key from the config or
//! `V0_API_KEY`, optionally scoped to a project: the billing allowance of the
//! current cycle and the request rate limit. v0 keeps no local sign-in to
//! find, and CodexBar reads no cookies for it either. The free account's
//! daily allowance and grace period are not shown, as in CodexBar.

use crate::text::Text;
use crate::{
    Result,
    {
        model::{Account, Kind, MONTH, Provider, Report, Section, Window},
        probe::{Probe, Request},
        service::{Meta, Service, Setting, Timestamp, json},
        values::{count, encode, invalid},
    },
};
use serde::Deserialize;

const BASE: &str = "https://api.v0.dev/v1";

pub(crate) struct V0;

static META: Meta = Meta::new("v0", "v0")
    .dashboard("https://v0.app/settings/billing")
    .settings(&[
        Setting::new("api_key", &["V0_API_KEY"], "usage-v0-setting-api-key"),
        Setting::new("scope", &["V0_SCOPE"], "usage-v0-setting-scope"),
    ]);

impl Service for V0 {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let key = probe.setting("api_key")?;
        let scope = probe
            .text_setting("scope")
            .filter(|scope| !scope.is_empty());
        let query = scope
            .as_deref()
            .map(|scope| format!("?scope={}", encode(scope)))
            .unwrap_or_default();
        let mut get =
            |path: &str| probe.body(Request::get(format!("{BASE}{path}{query}")).bearer(&key));
        let billing = match get("/user/billing") {
            Ok(body) => body,
            Err(error) => return Some(Err(error)),
        };
        Some(get("/rate-limits").and_then(|limits| parse(&billing, &limits, scope.as_deref())))
    }
}

pub(crate) fn parse(billing: &str, rate_limits: &str, scope: Option<&str>) -> Result<Report> {
    let billing: Billing = json(billing)?;
    let data = billing.data.ok_or_else(invalid)?;
    let (allowance, on_demand) = match billing.billing_type.as_deref().map(str::trim) {
        Some("token") => {
            let balance = data.balance.ok_or_else(invalid)?;
            let total = finite(balance.total)?;
            let remaining = finite(balance.remaining)?;
            if total < 0. {
                return Err(invalid());
            }
            let cycle = data.billing_cycle.ok_or_else(invalid)?;
            let on_demand = data
                .on_demand
                .map(|on_demand| finite(on_demand.balance))
                .transpose()?;
            let quota = Quota {
                limit: total,
                remaining: Some(remaining),
                reset: cycle.end,
            };
            (quota, on_demand)
        }
        Some("legacy") => {
            let quota = Quota {
                limit: finite(data.limit)?,
                remaining: data
                    .remaining
                    .map(|remaining| finite(Some(remaining)))
                    .transpose()?,
                reset: data.reset,
            };
            if quota.limit < 0. {
                return Err(invalid());
            }
            (quota, None)
        }
        _ => return Err(invalid()),
    };
    let limits: Plain = json(rate_limits)?;
    let rate = Quota {
        limit: finite(limits.limit)?,
        remaining: limits
            .remaining
            .map(|remaining| finite(Some(remaining)))
            .transpose()?,
        reset: limits.reset,
    };
    if rate.limit < 0. {
        return Err(invalid());
    }
    let windows = [
        allowance.window(Kind::Monthly, Some(MONTH)),
        rate.window(Kind::Named(Text::message("usage-v0-rate-limit")), None),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut facts = vec![(
        Text::message("usage-v0-billing-remaining"),
        allowance.describe(),
    )];
    if let Some(on_demand) = on_demand {
        facts.push((
            Text::message("usage-v0-on-demand-balance"),
            Text::service(count(on_demand)),
        ));
    }
    facts.push((
        Text::message("usage-v0-rate-limit-remaining"),
        rate.describe(),
    ));
    // v0's own billing type, such as token or legacy.
    if let Some(kind) = billing.billing_type {
        facts.push((
            Text::message("usage-v0-billing-type"),
            Text::service(kind.trim()),
        ));
    }
    if let Some(scope) = scope {
        facts.push((
            Text::message("usage-v0-scope"),
            Text::service(scope.chars().take(120).collect::<String>()),
        ));
    }
    Ok(
        Report::new(Provider(&V0), Account::default(), windows).with_sections([Section::Facts {
            title: Text::message("usage-v0-api"),
            facts,
        }]),
    )
}

struct Quota {
    limit: f64,
    remaining: Option<f64>,
    reset: Option<Timestamp>,
}

impl Quota {
    /// No window when the remaining amount is unknown: v0 then reports only
    /// the limit, and a percentage would be invented.
    fn window(&self, kind: Kind, length: Option<std::time::Duration>) -> Option<Window> {
        let remaining = self.remaining?;
        let used = if self.limit > 0. {
            (self.limit - remaining).max(0.) / self.limit * 100.
        } else {
            0.
        };
        Some(Window::new(
            kind,
            used,
            self.reset.as_ref().and_then(Timestamp::time),
            length,
        ))
    }

    fn describe(&self) -> Text {
        match self.remaining {
            Some(remaining) => Text::message("usage-v0-remaining-of")
                .arg("remaining", count(remaining))
                .arg("limit", count(self.limit)),
            None => Text::message("usage-v0-unavailable").arg("limit", count(self.limit)),
        }
    }
}

fn finite(value: Option<f64>) -> Result<f64> {
    value.filter(|value| value.is_finite()).ok_or_else(invalid)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Billing {
    billing_type: Option<String>,
    data: Option<BillingData>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingData {
    balance: Option<TokenBalance>,
    billing_cycle: Option<Cycle>,
    on_demand: Option<OnDemand>,
    limit: Option<f64>,
    remaining: Option<f64>,
    reset: Option<Timestamp>,
}

#[derive(Deserialize)]
struct TokenBalance {
    total: Option<f64>,
    remaining: Option<f64>,
}

#[derive(Deserialize)]
struct Cycle {
    end: Option<Timestamp>,
}

#[derive(Deserialize)]
struct OnDemand {
    balance: Option<f64>,
}

#[derive(Deserialize)]
struct Plain {
    limit: Option<f64>,
    remaining: Option<f64>,
    reset: Option<Timestamp>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const TOKEN: &str = r#"{"billingType":"token","data":{"balance":{"remaining":750,"total":1000},
        "billingCycle":{"end":1800003600},"onDemand":{"balance":120}}}"#;
    const LIMITS: &str = r#"{"limit":100,"remaining":80,"reset":1800001800}"#;

    #[test]
    fn token_billing_and_rate_limit_become_windows() {
        let report = parse(TOKEN, LIMITS, Some("project-demo")).unwrap();
        assert_eq!(report.windows.len(), 2);
        let billing = &report.windows[0];
        assert_eq!(billing.kind, Kind::Monthly);
        assert!((billing.used - 25.).abs() < 0.01);
        assert!(billing.resets_at.is_some());
        let rate = &report.windows[1];
        assert_eq!(rate.kind, Kind::Named(Text::message("usage-v0-rate-limit")));
        assert!((rate.used - 20.).abs() < 0.01);
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        assert_eq!(
            facts,
            &[
                (
                    Text::message("usage-v0-billing-remaining"),
                    Text::message("usage-v0-remaining-of")
                        .arg("remaining", "750")
                        .arg("limit", "1,000")
                ),
                (
                    Text::message("usage-v0-on-demand-balance"),
                    Text::service("120")
                ),
                (
                    Text::message("usage-v0-rate-limit-remaining"),
                    Text::message("usage-v0-remaining-of")
                        .arg("remaining", "80")
                        .arg("limit", "100")
                ),
                (
                    Text::message("usage-v0-billing-type"),
                    Text::service("token")
                ),
                (
                    Text::message("usage-v0-scope"),
                    Text::service("project-demo")
                ),
            ]
        );
    }

    #[test]
    fn legacy_without_remaining_invents_no_percent() {
        let report = parse(
            r#"{"billingType":"legacy","data":{"limit":1000}}"#,
            r#"{"limit":100}"#,
            None,
        )
        .unwrap();
        assert!(report.windows.is_empty());
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        assert_eq!(
            facts[0].1,
            Text::message("usage-v0-unavailable").arg("limit", "1,000")
        );
    }

    #[test]
    fn unknown_billing_type_fails() {
        assert!(parse(r#"{"billingType":"credits","data":{}}"#, LIMITS, None).is_err());
        assert!(parse(TOKEN, r#"{"remaining":1}"#, None).is_err());
    }

    #[test]
    fn scope_is_percent_encoded() {
        assert_eq!(encode("team/my project"), "team%2Fmy%20project");
    }
}
