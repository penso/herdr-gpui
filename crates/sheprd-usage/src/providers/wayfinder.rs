//! Wayfinder local router health, routing split, and savings, read from the
//! gateway's unauthenticated read-only endpoints on the probed host
//! (`http://127.0.0.1:8088` unless the `base_url` setting or
//! `WAYFINDER_GATEWAY_URL` names another). There is no sign-in: a gateway
//! that answers `/healthz` is the detection, so nothing is shown while it is
//! not running unless a URL is configured. Everything CodexBar reads is
//! ported; its "open dashboard" link follows the configured URL, which a
//! fixed dashboard link cannot, so none is given.

use crate::{
    Error, Result,
    {
        model::{Account, Balance, Provider, Report, Section, Unit, group},
        probe::{Probe, Request},
        service::{Meta, Service, Setting, json},
        text::Text,
    },
};
use serde::{Deserialize, de::IgnoredAny};
use std::{collections::HashMap, time::Duration};

const DEFAULT: &str = "http://127.0.0.1:8088";
const TIMEOUT: Duration = Duration::from_secs(5);
/// Looking for a gateway that is usually not running must stay cheap.
const DETECT_TIMEOUT: Duration = Duration::from_secs(2);
const LATENCY: &str = "wayfinder_router_decision_latency_seconds";

pub(crate) struct Wayfinder;

static META: Meta = Meta::new("wayfinder", "Wayfinder").settings(&[Setting::new(
    "base_url",
    &["WAYFINDER_GATEWAY_URL"],
    "usage-wayfinder-setting-base-url",
)]);

impl Service for Wayfinder {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let configured = probe.text_setting("base_url").filter(|raw| !raw.is_empty());
        let Some(raw) = configured else {
            let health = probe
                .http(Request::get(format!("{DEFAULT}/healthz")).timeout(DETECT_TIMEOUT))
                .ok()
                .filter(|response| response.status == 200)?;
            json::<Health>(&health.body).ok()?;
            return Some(fetch(probe, DEFAULT, Some(health.body)));
        };
        Some(
            gateway(&raw)
                .ok_or(Error::NotSignedIn)
                .and_then(|base| fetch(probe, &base, None)),
        )
    }
}

fn fetch(probe: &mut Probe, base: &str, health: Option<String>) -> Result<Report> {
    let mut get = |path: &str| probe.body(Request::get(format!("{base}/{path}")).timeout(TIMEOUT));
    let health = match health {
        Some(health) => health,
        None => get("healthz")?,
    };
    let models = get("router/models")?;
    let savings = get("v1/savings?period=30d")?;
    // Latency is a nicety; the report never fails for want of it.
    let metrics = get("metrics").ok();
    parse(&health, &models, &savings, metrics.as_deref())
}

/// The gateway URL without a trailing slash. HTTPS, or plain HTTP only on a
/// loopback address, since the gateway is a local service.
fn gateway(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_end_matches('/');
    let url = url::Url::parse(raw).ok()?;
    let loopback = match url.host()? {
        url::Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
    };
    let allowed = url.scheme() == "https" || (url.scheme() == "http" && loopback);
    (allowed && url.username().is_empty() && url.password().is_none()).then(|| raw.to_owned())
}

fn parse(health: &str, models: &str, savings: &str, metrics: Option<&str>) -> Result<Report> {
    let health: Health = json(health)?;
    let models: Models = json(models)?;
    let savings: Savings = json(savings)?;
    let missing = health.missing_keys.map_or(0, |keys| keys.len());
    let status = if health.offline {
        Text::message("usage-wayfinder-offline")
    } else if models.dry_run {
        Text::message("usage-wayfinder-dry-run")
    } else if health.status == "degraded" {
        Text::message("usage-wayfinder-degraded").arg("missing", missing)
    } else {
        Text::message("usage-wayfinder-local-gateway")
    };
    let model_count = models.models.len();
    // The gateway's own health word, such as `ok` or `degraded`.
    let mode = match (health.offline, models.dry_run) {
        (false, false) => "live",
        (true, false) => "offline",
        (false, true) => "dry-run",
        (true, true) => "both",
    };
    let gateway = Text::message("usage-wayfinder-gateway-state")
        .arg("status", health.status.as_str())
        .arg("models", model_count)
        .arg("mode", mode);

    let mut routes: Vec<(String, Route)> = savings.by_route.into_iter().collect();
    routes.sort_by(|a, b| b.1.requests.cmp(&a.1.requests).then_with(|| a.0.cmp(&b.0)));
    let mut facts = vec![(Text::message("usage-wayfinder-gateway"), gateway)];
    if savings.requests > 0 && !routes.is_empty() {
        // Route names are the gateway's, as are the separators between them.
        facts.push((
            Text::message("usage-wayfinder-routed"),
            Text::service(
                routes
                    .iter()
                    .take(5)
                    .map(|(name, route)| format!("{name}: {}", group(route.requests)))
                    .collect::<Vec<_>>()
                    .join(" · "),
            ),
        ));
    }
    let saved = savings.requests > 0 && savings.saved > 0.;
    if saved {
        let percent = if savings.saved_pct.fract() == 0. {
            format!("{:.0}", savings.saved_pct)
        } else {
            format!("{:.1}", savings.saved_pct)
        };
        // Unpriced gateways save in relative units, never dollars.
        let value = match (savings.priced, savings.saved < 0.01) {
            (false, _) => Text::message("usage-wayfinder-saved-relative").arg("percent", percent),
            (true, true) => Text::message("usage-wayfinder-saved-value")
                .arg("amount", "<$0.01")
                .arg("percent", percent),
            (true, false) => Text::message("usage-wayfinder-saved-value")
                .arg("amount", format!("${:.2}", savings.saved))
                .arg("percent", percent),
        };
        facts.push((Text::message("usage-wayfinder-saved"), value));
    }
    if let Some(latency) = metrics.and_then(average_decision_ms) {
        facts.push((
            Text::message("usage-wayfinder-decision"),
            Text::message("usage-wayfinder-milliseconds").arg("time", format!("{latency:.1}")),
        ));
    }
    let shares = (savings.requests > 0).then(|| Section::Shares {
        title: Text::message("usage-wayfinder-requests-by-route"),
        shares: routes
            .iter()
            .take(5)
            .map(|(name, route)| {
                (
                    Text::service(name),
                    (route.requests.max(0) as f64 / savings.requests as f64 * 100.) as f32,
                )
            })
            .collect(),
    });
    Ok(Report::new(
        Provider(&Wayfinder),
        Account {
            email: Some(Text::message("usage-wayfinder-account").arg("models", model_count)),
            plan: Some(status),
        },
        Vec::new(),
    )
    .with_balances((saved && savings.priced).then(|| {
        Balance::new(
            Text::message("usage-wayfinder-saved"),
            savings.saved,
            Unit::Currency("USD".into()),
        )
    }))
    .with_sections(
        [Section::Facts {
            title: Text::message("usage-report-usage"),
            facts,
        }]
        .into_iter()
        .chain(shares),
    ))
}

/// The mean routing decision time from the Prometheus histogram's sum and
/// count, in milliseconds.
fn average_decision_ms(text: &str) -> Option<f64> {
    let value = |line: &str, name: &str| {
        let rest = line.strip_prefix(name)?;
        if !(rest.starts_with(' ') || rest.starts_with('{')) {
            return None;
        }
        rest.split(' ').next_back()?.trim().parse::<f64>().ok()
    };
    let (sum_name, count_name) = (format!("{LATENCY}_sum"), format!("{LATENCY}_count"));
    let (mut sum, mut count) = (None, None);
    for line in text.lines() {
        if let Some(found) = value(line, &sum_name) {
            sum = Some(found);
        } else if let Some(found) = value(line, &count_name) {
            count = Some(found);
        }
    }
    let count = count.filter(|count| *count > 0.)?;
    Some(sum? / count * 1000.)
}

#[derive(Deserialize)]
struct Health {
    status: String,
    offline: bool,
    missing_keys: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Models {
    models: Vec<IgnoredAny>,
    dry_run: bool,
}

#[derive(Deserialize)]
struct Savings {
    priced: bool,
    requests: i64,
    saved: f64,
    saved_pct: f64,
    by_route: HashMap<String, Route>,
}

#[derive(Deserialize)]
struct Route {
    requests: i64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const HEALTH: &str =
        r#"{"status":"degraded","offline":false,"missing_keys":["OPENAI_API_KEY"]}"#;
    const MODELS: &str = r#"{"models":[{"name":"local"},{"name":"cloud"}],"dry_run":false}"#;
    const SAVINGS: &str = r#"{"priced":true,"requests":14,"tokens":42000,"realized":1.5,
        "baseline":5.62,"saved":4.12,"saved_pct":73.3,
        "by_route":{"cloud":{"requests":4,"saved":0,"tokens":20000},
                    "local":{"requests":10,"saved":4.12,"tokens":22000}}}"#;
    const METRICS: &str = "# HELP wayfinder_router_decision_latency_seconds Decision time\n\
        wayfinder_router_decision_latency_seconds_bucket{le=\"0.001\"} 20\n\
        wayfinder_router_decision_latency_seconds_sum 0.004\n\
        wayfinder_router_decision_latency_seconds_count 20\n";

    #[test]
    fn reads_gateway_health_routes_and_savings() {
        let report = parse(HEALTH, MODELS, SAVINGS, Some(METRICS)).unwrap();
        assert!(report.windows.is_empty());
        assert_eq!(
            report.account.email,
            Some(Text::message("usage-wayfinder-account").arg("models", 2))
        );
        assert_eq!(
            report.account.plan,
            Some(Text::message("usage-wayfinder-degraded").arg("missing", 1))
        );
        assert_eq!(report.balances[0].text(), "$4.12");
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        assert_eq!(
            facts,
            &[
                (
                    Text::message("usage-wayfinder-gateway"),
                    Text::message("usage-wayfinder-gateway-state")
                        .arg("status", "degraded")
                        .arg("models", 2)
                        .arg("mode", "live")
                ),
                (
                    Text::message("usage-wayfinder-routed"),
                    Text::service("local: 10 · cloud: 4")
                ),
                (
                    Text::message("usage-wayfinder-saved"),
                    Text::message("usage-wayfinder-saved-value")
                        .arg("amount", "$4.12")
                        .arg("percent", "73.3")
                ),
                (
                    Text::message("usage-wayfinder-decision"),
                    Text::message("usage-wayfinder-milliseconds").arg("time", "0.2")
                ),
            ]
        );
        let Section::Shares { shares, .. } = &report.sections[1] else {
            panic!("expected shares");
        };
        assert_eq!(shares[0].0, "local");
    }

    #[test]
    fn unpriced_savings_are_relative() {
        let savings = r#"{"priced":false,"requests":2,"tokens":10,"realized":1,"baseline":2,
            "saved":1,"saved_pct":50,"by_route":{"local":{"requests":2,"saved":1,"tokens":10}}}"#;
        let report = parse(
            r#"{"status":"ok","offline":true}"#,
            r#"{"models":[{"name":"local"}],"dry_run":true}"#,
            savings,
            None,
        )
        .unwrap();
        assert!(report.balances.is_empty());
        assert_eq!(
            report.account.plan,
            Some(Text::message("usage-wayfinder-offline"))
        );
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        assert_eq!(
            facts[0].1,
            Text::message("usage-wayfinder-gateway-state")
                .arg("status", "ok")
                .arg("models", 1)
                .arg("mode", "both")
        );
        assert_eq!(
            facts[2].1,
            Text::message("usage-wayfinder-saved-relative").arg("percent", "50")
        );
    }

    #[test]
    fn gateway_urls_are_https_or_loopback() {
        assert_eq!(
            gateway("http://localhost:9000/").as_deref(),
            Some("http://localhost:9000")
        );
        assert_eq!(
            gateway("https://router.example.com").as_deref(),
            Some("https://router.example.com")
        );
        assert_eq!(gateway("http://router.example.com"), None);
        assert_eq!(gateway("http://user:pw@127.0.0.1:8088"), None);
    }

    #[test]
    fn latency_needs_a_count() {
        assert_eq!(
            average_decision_ms("wayfinder_router_decision_latency_seconds_sum 1\n"),
            None
        );
    }
}
