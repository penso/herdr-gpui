//! Bifrost gateway budgets and rate limits from the self-service virtual-key
//! quota endpoint, read with the virtual key (the `api_key` setting or
//! `BIFROST_API_KEY`, here or on the probed host) at the configured
//! `base_url`. No admin credential is used. Everything CodexBar reads is
//! ported; the per-budget list and model rows are shown as facts.

use crate::{
    Error, Result,
    usage::{
        model::{Account, Balance, Kind, Provider, Report, Section, Unit, Window},
        probe::{Probe, Request, Secret},
        service::{Meta, Service, Setting, Timestamp, json},
        values,
    },
};
use serde::Deserialize;
use std::{
    cmp::Ordering,
    time::{Duration, SystemTime},
};

const QUOTA_PATH: &str = "/api/governance/virtual-keys/quota";
/// Model rows shown before the rest fold into "Other models".
const TOP_MODELS: usize = 5;

pub(crate) struct Bifrost;

static META: Meta = Meta::new("bifrost", "Bifrost").settings(&[
    Setting::new(
        "api_key",
        &["BIFROST_API_KEY"],
        "Your Bifrost virtual key (vk-…), from the gateway's Virtual Keys page. It is \
             sent as the x-bf-vk header to base_url only.",
    ),
    Setting::new(
        "base_url",
        &["BIFROST_BASE_URL"],
        "Your Bifrost gateway's URL, e.g. https://bifrost.example.com. Bifrost has no \
             public host. It must be HTTPS unless it is on localhost, a private network, \
             or a .local host.",
    ),
]);

impl Service for Bifrost {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let key = probe
            .setting("api_key")
            .or_else(|| probe.env("BIFROST_API_KEY"))?;
        Some(fetch(probe, &key))
    }
}

fn fetch(probe: &mut Probe, key: &Secret) -> Result<Report> {
    let base = probe
        .text_setting("base_url")
        .filter(|base| !base.is_empty())
        .ok_or(Error::UsageNotSignedIn)?;
    let request = Request::get(quota_url(&base)?)
        .secret_header("x-bf-vk", "", key)
        .header("Accept", "application/json");
    let body = probe.body(request)?;
    parse(&body, SystemTime::now())
}

fn quota_url(raw: &str) -> Result<String> {
    let mut url = values::gateway(raw)?;
    let path = format!("{}{QUOTA_PATH}", url.path().trim_end_matches('/'));
    url.set_path(&path);
    url.set_fragment(None);
    Ok(url.to_string())
}

fn text(value: Option<&String>) -> Option<&str> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

pub(crate) fn parse(body: &str, now: SystemTime) -> Result<Report> {
    let root: Root = json(body)?;
    let mut scopes: Vec<(Option<String>, &Limits)> = vec![(None, &root.limits)];
    for (index, config) in root.provider_configs.iter().enumerate() {
        let name =
            text(config.provider.as_ref()).map_or_else(|| (index + 1).to_string(), str::to_owned);
        scopes.push((Some(format!("Provider {name}")), &config.limits));
    }
    for (index, config) in root.model_configs.iter().enumerate() {
        let model =
            text(config.model_name.as_ref()).map_or_else(|| (index + 1).to_string(), str::to_owned);
        let name = match text(config.provider.as_ref()) {
            Some(provider) => format!("{provider} · {model}"),
            None => model,
        };
        scopes.push((Some(format!("Model {name}")), &config.limits));
    }

    let mut budgets: Vec<Budget> = Vec::new();
    for (scope, limits) in &scopes {
        for row in &limits.budgets {
            let Some(id) = text(row.id.as_ref()) else {
                continue;
            };
            let amount = row.override_amount.unwrap_or(0.);
            let cycles = row.override_cycles_remaining.unwrap_or(0.);
            let active = amount > 0.
                && match text(row.override_mode.as_ref()) {
                    Some("forever") => true,
                    Some("cycles") => cycles > 0.,
                    _ => false,
                };
            let limit = row.max_limit.unwrap_or(0.) + if active { amount } else { 0. };
            if !limit.is_finite() {
                return Err(values::invalid());
            }
            budgets.push(Budget {
                id: id.to_owned(),
                scope: scope.clone(),
                source: text(row.source_name.as_ref()).map(str::to_owned),
                limit,
                used: row.current_usage.unwrap_or(0.),
                timing: Timing::new(
                    text(row.reset_duration.as_ref()),
                    text(row.last_reset.as_ref()),
                    now,
                ),
                models: &row.per_model_usage,
            });
        }
    }
    budgets.sort_by(|a, b| {
        let seconds = |budget: &Budget| budget.timing.seconds.unwrap_or(f64::INFINITY);
        seconds(a)
            .partial_cmp(&seconds(b))
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut rate_limits = Vec::new();
    let mut unknown_limits = Vec::new();
    for (scope, limits) in &scopes {
        // `rate_limit` merges the components; it is no separate pool.
        let selected: Vec<&RateLimit> = if limits.rate_limits.is_empty() {
            limits.rate_limit.iter().collect()
        } else {
            limits.rate_limits.iter().collect()
        };
        for limit in selected {
            for (dimension, max, used, reset, last) in [
                (
                    "Tokens",
                    limit.token_max_limit,
                    limit.token_current_usage,
                    &limit.token_reset_duration,
                    &limit.token_last_reset,
                ),
                (
                    "Requests",
                    limit.request_max_limit,
                    limit.request_current_usage,
                    &limit.request_reset_duration,
                    &limit.request_last_reset,
                ),
            ] {
                let known = max.is_some_and(|max| max > 0.);
                let reset = text(reset.as_ref());
                // Last-reset times come even for dimensions nobody configured.
                if !known && reset.is_none() {
                    continue;
                }
                let title = [
                    scope.as_deref(),
                    text(limit.source_name.as_ref()),
                    Some(dimension),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
                let timing = Timing::new(reset, text(last.as_ref()), now);
                match max.filter(|_| known) {
                    Some(max) => rate_limits.push(Section::Limit(Window::new(
                        Kind::Named(title),
                        used.unwrap_or(0.) / max * 100.,
                        timing.resets_at,
                        timing.length,
                    ))),
                    None => unknown_limits.push((title, "Unavailable".to_owned())),
                }
            }
        }
    }
    let inactive = root.is_active == Some(false);
    if inactive && budgets.is_empty() && rate_limits.is_empty() && unknown_limits.is_empty() {
        return Err(Error::UsageRejected);
    }

    let mut windows = Vec::new();
    let mut scoped = Vec::new();
    for budget in budgets.iter().filter(|budget| budget.limit > 0.) {
        let window = budget.window();
        if budget.scope.is_some() {
            scoped.push(Section::Limit(window));
        } else {
            windows.push(window);
        }
    }
    let first = budgets.iter().find(|budget| budget.scope.is_none());
    let balance = first.map(|budget| {
        let label = budget.timing.label.map_or_else(
            || if budget.limit > 0. { "Budget" } else { "Spend" }.to_owned(),
            str::to_owned,
        );
        let balance = Balance::new(label, budget.used, Unit::Currency("USD".into()));
        if budget.limit > 0. {
            balance.out_of(budget.limit)
        } else {
            balance
        }
    });

    let mut sections = scoped;
    sections.extend(rate_limits);
    if !unknown_limits.is_empty() {
        sections.push(Section::Facts {
            title: "Rate limits".into(),
            facts: unknown_limits,
        });
    }
    if inactive {
        sections.insert(
            0,
            Section::Facts {
                title: "Virtual key".into(),
                facts: vec![("Status".into(), "Inactive".into())],
            },
        );
    }
    if let Some(models) = first.and_then(|budget| models(budget.models)) {
        sections.push(models);
    }
    if budgets.len() > 1 || budgets.iter().any(|budget| budget.scope.is_some()) {
        sections.push(Section::Facts {
            title: "Budgets".into(),
            facts: budgets.iter().take(24).map(Budget::fact).collect(),
        });
    }
    let account = Account {
        email: text(root.virtual_key_name.as_ref()).map(str::to_owned),
        plan: first.and_then(|budget| budget.source.clone()),
    };
    Ok(Report::new(Provider(&Bifrost), account, windows)
        .with_balances(balance)
        .with_sections(sections))
}

struct Budget<'a> {
    id: String,
    scope: Option<String>,
    source: Option<String>,
    limit: f64,
    used: f64,
    timing: Timing,
    models: &'a [ModelUsage],
}

impl Budget<'_> {
    fn title(&self) -> String {
        [
            self.scope.as_deref(),
            Some(self.source.as_deref().unwrap_or("Budget")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }

    fn window(&self) -> Window {
        let kind = match (self.timing.label, &self.source, &self.scope) {
            (Some("Daily"), None, None) => Kind::Daily,
            (Some("Weekly"), None, None) => Kind::Weekly,
            (Some("Monthly"), None, None) => Kind::Monthly,
            (label, _, _) => Kind::Named(
                [Some(self.title()), label.map(str::to_owned)]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · "),
            ),
        };
        let length = self.timing.length.or_else(|| kind.length());
        Window::new(
            kind,
            self.used / self.limit * 100.,
            self.timing.resets_at,
            length,
        )
    }

    fn fact(&self) -> (String, String) {
        let label = match self.source {
            Some(_) => self.title(),
            None => format!("{} {}", self.title(), self.id),
        };
        let mut value = if self.limit > 0. {
            format!("${:.2} / ${:.2}", self.used, self.limit)
        } else {
            format!("${:.2}", self.used)
        };
        if let Some(period) = self.timing.label {
            value.push_str(" · ");
            value.push_str(period);
        }
        (label, value)
    }
}

/// When a budget or limit resets. Calendar periods (`1d`, `1M`, …) depend on
/// an alignment policy the answer omits, so only fixed durations such as
/// `1h30m` get a reset time and a length.
struct Timing {
    seconds: Option<f64>,
    resets_at: Option<SystemTime>,
    length: Option<Duration>,
    label: Option<&'static str>,
}

impl Timing {
    fn new(raw: Option<&str>, last: Option<&str>, now: SystemTime) -> Self {
        let seconds = raw.and_then(duration);
        let fixed = raw.is_some_and(|raw| !raw.ends_with(['d', 'w', 'M', 'Q', 'Y']));
        let length = seconds
            .filter(|seconds| fixed && *seconds >= 60.)
            .map(|seconds| Duration::from_secs((seconds / 60.).floor() as u64 * 60));
        let resets_at = seconds.filter(|_| fixed).and_then(|seconds| {
            let start = Timestamp::Text(last?.to_owned()).time()?;
            let elapsed = now.duration_since(start).unwrap_or_default().as_secs_f64();
            let cycles = (elapsed / seconds).floor() + 1.;
            start.checked_add(Duration::try_from_secs_f64(cycles * seconds).ok()?)
        });
        let label = match raw {
            Some("1h") => Some("Hourly"),
            Some("1d") => Some("Daily"),
            Some("1w") => Some("Weekly"),
            Some("1M") => Some("Monthly"),
            Some("1Q") => Some("Quarterly"),
            Some("1Y") => Some("Yearly"),
            _ => None,
        };
        Self {
            seconds,
            resets_at,
            length,
            label,
        }
    }
}

/// Bifrost's durations: a calendar count such as `1M`, or a Go duration
/// such as `1h30m`.
fn duration(raw: &str) -> Option<f64> {
    const CALENDAR: [(char, f64); 5] = [
        ('d', 86_400.),
        ('w', 604_800.),
        ('M', 2_592_000.),
        ('Q', 7_776_000.),
        ('Y', 31_536_000.),
    ];
    const GO: [(&str, f64); 8] = [
        ("ns", 1e-9),
        ("us", 1e-6),
        ("µs", 1e-6),
        ("μs", 1e-6),
        ("ms", 1e-3),
        ("s", 1.),
        ("m", 60.),
        ("h", 3600.),
    ];
    let number_end = |text: &str| {
        text.find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(text.len())
    };
    let seconds = if let Some((unit, scale)) = CALENDAR
        .iter()
        .find(|(unit, _)| raw.ends_with(*unit))
        .copied()
    {
        let count = &raw[..raw.len() - unit.len_utf8()];
        if count.is_empty() || number_end(count) != count.len() {
            return None;
        }
        count.parse::<f64>().ok()? * scale
    } else {
        let mut rest = raw;
        let mut total = 0.;
        while !rest.is_empty() {
            let end = number_end(rest);
            let value: f64 = rest[..end].parse().ok()?;
            rest = &rest[end..];
            // Longest unit first, so "ms" is not read as "m".
            let (unit, scale) = GO
                .iter()
                .filter(|(unit, _)| rest.starts_with(unit))
                .max_by_key(|(unit, _)| unit.len())?;
            rest = &rest[unit.len()..];
            total += value * scale;
        }
        total
    };
    (seconds.is_finite() && seconds > 0.).then_some(seconds)
}

/// The key-wide budget's model spend, costliest first.
fn models(rows: &[ModelUsage]) -> Option<Section> {
    let mut rows: Vec<(&str, f64, f64)> = rows
        .iter()
        .map(|row| {
            (
                text(row.model.as_ref()).unwrap_or("Model"),
                row.total_cost.unwrap_or(0.),
                row.total_tokens.unwrap_or(0.),
            )
        })
        .filter(|(_, cost, tokens)| *cost != 0. || *tokens != 0.)
        .collect();
    if rows.is_empty() {
        return None;
    }
    rows.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(Ordering::Equal)
            .then(b.2.partial_cmp(&a.2).unwrap_or(Ordering::Equal))
            .then(a.0.cmp(b.0))
    });
    let mut facts: Vec<(String, String)> = rows
        .iter()
        .take(TOP_MODELS)
        .map(|(model, cost, tokens)| {
            (
                model_name(model),
                format!("${cost:.2} · {} tokens", token_count(*tokens)),
            )
        })
        .collect();
    if rows.len() > TOP_MODELS {
        facts.push(("Other models".into(), (rows.len() - TOP_MODELS).to_string()));
    }
    Some(Section::Facts {
        title: "Models".into(),
        facts,
    })
}

/// Strips Bedrock region and vendor prefixes and revision suffixes, which
/// Bifrost reports verbatim, e.g. `us.anthropic.claude-sonnet-4-v1:0`.
fn model_name(raw: &str) -> String {
    const REGIONS: [&str; 5] = ["us-gov.", "us.", "eu.", "apac.", "global."];
    const VENDORS: [&str; 13] = [
        "ai21.",
        "amazon.",
        "anthropic.",
        "cohere.",
        "deepseek.",
        "luma.",
        "meta.",
        "mistral.",
        "openai.",
        "qwen.",
        "stability.",
        "twelvelabs.",
        "writer.",
    ];
    let strip = |name: &str, prefixes: &[&str]| -> String {
        prefixes
            .iter()
            .find(|prefix| {
                name.get(..prefix.len())
                    .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
            })
            .map_or_else(|| name.to_owned(), |prefix| name[prefix.len()..].to_owned())
    };
    let mut name = strip(&strip(raw, &REGIONS[..]), &VENDORS[..]);
    if let Some(index) = name.rfind("-v")
        && name[index + 2..]
            .split_once(':')
            .is_some_and(|(major, minor)| {
                !major.is_empty()
                    && !minor.is_empty()
                    && major.chars().all(|c| c.is_ascii_digit())
                    && minor.chars().all(|c| c.is_ascii_digit())
            })
    {
        name.truncate(index);
    }
    if name.is_empty() {
        raw.to_owned()
    } else {
        name
    }
}

fn token_count(value: f64) -> String {
    let magnitude = value.abs();
    for (threshold, divisor, unit) in [
        (999_500_000., 1e9, "B"),
        (999_500., 1e6, "M"),
        (1000., 1e3, "K"),
    ] {
        if magnitude >= threshold {
            let scaled = value / divisor;
            let digits = if scaled.abs() >= 10. { 0 } else { 1 };
            let text = format!("{scaled:.digits$}");
            let text = text.strip_suffix(".0").unwrap_or(&text);
            return format!("{text}{unit}");
        }
    }
    format!("{}", value.trunc())
}

#[derive(Deserialize)]
struct Root {
    is_active: Option<bool>,
    virtual_key_name: Option<String>,
    #[serde(flatten)]
    limits: Limits,
    #[serde(default, deserialize_with = "list")]
    provider_configs: Vec<ScopeConfig>,
    #[serde(default, deserialize_with = "list")]
    model_configs: Vec<ScopeConfig>,
}

#[derive(Deserialize)]
struct ScopeConfig {
    provider: Option<String>,
    model_name: Option<String>,
    #[serde(flatten)]
    limits: Limits,
}

#[derive(Deserialize)]
struct Limits {
    #[serde(default, deserialize_with = "list")]
    budgets: Vec<BudgetRow>,
    rate_limit: Option<RateLimit>,
    #[serde(default, deserialize_with = "list")]
    rate_limits: Vec<RateLimit>,
}

#[derive(Deserialize)]
struct BudgetRow {
    id: Option<String>,
    max_limit: Option<f64>,
    current_usage: Option<f64>,
    reset_duration: Option<String>,
    last_reset: Option<String>,
    source_name: Option<String>,
    override_amount: Option<f64>,
    override_mode: Option<String>,
    override_cycles_remaining: Option<f64>,
    #[serde(default, deserialize_with = "list")]
    per_model_usage: Vec<ModelUsage>,
}

#[derive(Deserialize)]
struct RateLimit {
    source_name: Option<String>,
    token_max_limit: Option<f64>,
    token_current_usage: Option<f64>,
    token_reset_duration: Option<String>,
    token_last_reset: Option<String>,
    request_max_limit: Option<f64>,
    request_current_usage: Option<f64>,
    request_reset_duration: Option<String>,
    request_last_reset: Option<String>,
}

#[derive(Deserialize)]
struct ModelUsage {
    model: Option<String>,
    total_cost: Option<f64>,
    total_tokens: Option<f64>,
}

/// A list that Bifrost may send as `null`.
fn list<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const QUOTA: &str = r#"{"virtual_key_name":"fixture-team","budgets":[
      {"id":"year","max_limit":1000,"current_usage":100,"reset_duration":"1Y"},
      {"id":"month","max_limit":125,"current_usage":42.17,"reset_duration":"1M",
       "last_reset":"2026-09-01T00:00:00Z","source_name":"Engineering",
       "per_model_usage":[
         {"model":"gpt-4o","provider":"openai","total_cost":5,"total_tokens":1200000},
         {"model":"fixture-empty","provider":"openai","total_cost":0,"total_tokens":0}]}],
     "rate_limit":{"id":"rl_1","token_max_limit":1000000,"token_current_usage":345678,
                   "token_reset_duration":"1d","request_max_limit":5000,"request_current_usage":120},
     "rate_limits":[{"id":"rl_1","token_max_limit":1000000,"token_current_usage":345678,
                     "token_reset_duration":"1d","request_max_limit":5000,"request_current_usage":120},
                    {"id":"rl_2","source_name":"Team pool","token_max_limit":200000,
                     "token_current_usage":100,"request_reset_duration":"1h"}]}"#;

    fn now() -> SystemTime {
        Timestamp::Text("2026-09-24T12:00:00Z".into())
            .time()
            .unwrap()
    }

    #[test]
    fn parses_budgets_limits_and_models() {
        let report = parse(QUOTA, now()).unwrap();
        assert_eq!(report.account.email.as_deref(), Some("fixture-team"));
        assert_eq!(report.account.plan.as_deref(), Some("Engineering"));
        assert_eq!(report.windows.len(), 2);
        let month = report
            .windows
            .iter()
            .find(|window| window.kind == Kind::Named("Engineering · Monthly".into()))
            .unwrap();
        assert!((month.used - 33.736).abs() < 0.001);
        assert_eq!(month.resets_at, None);
        assert_eq!(report.balances[0].label, "Monthly");
        assert_eq!(report.balances[0].amount, 42.17);
        assert_eq!(report.balances[0].total, Some(125.));
        let limits: Vec<_> = report
            .sections
            .iter()
            .filter_map(|section| match section {
                Section::Limit(window) => Some((window.kind.title().to_owned(), window.percent())),
                _ => None,
            })
            .collect();
        assert_eq!(
            limits,
            [
                ("Tokens".to_owned(), 35),
                ("Requests".to_owned(), 2),
                ("Team pool Tokens".to_owned(), 0),
            ]
        );
        let facts = |title: &str| {
            report.sections.iter().find_map(|section| match section {
                Section::Facts { title: t, facts } if t == title => Some(facts.clone()),
                _ => None,
            })
        };
        assert_eq!(
            facts("Rate limits").unwrap(),
            [("Team pool Requests".to_owned(), "Unavailable".to_owned())]
        );
        assert_eq!(
            facts("Models").unwrap(),
            [("gpt-4o".to_owned(), "$5.00 · 1.2M tokens".to_owned())]
        );
        assert_eq!(facts("Budgets").unwrap().len(), 2);
    }

    #[test]
    fn overrides_apply_only_while_active() {
        for (mode, cycles, limit) in [
            ("forever", 0, 150.),
            ("cycles", 2, 150.),
            ("cycles", 0, 100.),
            ("paused", 2, 100.),
        ] {
            let body = format!(
                r#"{{"budgets":[{{"id":"b1","max_limit":100,"current_usage":200,
                "override_amount":50,"override_mode":"{mode}","override_cycles_remaining":{cycles}}}]}}"#
            );
            let report = parse(&body, now()).unwrap();
            assert_eq!(report.windows[0].percent(), 100);
            assert_eq!(report.balances[0].total, Some(limit));
            assert_eq!(report.balances[0].amount, 200.);
        }
    }

    #[test]
    fn unlimited_budget_keeps_spend_without_window() {
        let report = parse(
            r#"{"budgets":[{"id":"unlimited","max_limit":0,"current_usage":5}]}"#,
            now(),
        )
        .unwrap();
        assert!(report.windows.is_empty());
        assert_eq!(report.balances[0].label, "Spend");
        assert_eq!(report.balances[0].total, None);
        let empty = parse(r#"{"virtual_key_name":"svc","budgets":null}"#, now()).unwrap();
        assert!(empty.balances.is_empty());
    }

    #[test]
    fn inactive_key_without_quotas_is_rejected() {
        assert!(matches!(
            parse(r#"{"is_active":false,"budgets":null}"#, now()),
            Err(Error::UsageRejected)
        ));
        let report = parse(
            r#"{"is_active":false,"budgets":[{"id":"u","max_limit":0,"current_usage":5}]}"#,
            now(),
        )
        .unwrap();
        assert!(
            matches!(&report.sections[0], Section::Facts { title, .. } if title == "Virtual key")
        );
    }

    #[test]
    fn fixed_durations_reset_on_schedule() {
        let body = r#"{"budgets":[{"id":"b1","max_limit":100,"current_usage":1,
            "reset_duration":"1h30m","last_reset":"2026-09-01T00:00:00Z"}]}"#;
        let window = &parse(body, now()).unwrap().windows[0];
        assert_eq!(window.length, Some(Duration::from_secs(90 * 60)));
        assert!(window.resets_at.unwrap() > now());
        assert_eq!(duration("1d"), Some(86_400.));
        assert_eq!(duration("1.5h"), Some(5400.));
        assert_eq!(duration("100ms"), Some(0.1));
        for bad in ["0s", "-1h", "invalid", "1d2h", ""] {
            assert_eq!(duration(bad), None, "{bad}");
        }
    }

    #[test]
    fn normalizes_model_names() {
        assert_eq!(
            model_name("us.anthropic.claude-sonnet-4-20250514-v1:0"),
            "claude-sonnet-4-20250514"
        );
        assert_eq!(model_name("gpt-4o"), "gpt-4o");
        assert_eq!(token_count(1_200_000.), "1.2M");
        assert_eq!(token_count(15_000.), "15K");
    }

    #[test]
    fn builds_quota_url() {
        assert_eq!(
            quota_url("https://bifrost.example.com/").unwrap(),
            "https://bifrost.example.com/api/governance/virtual-keys/quota"
        );
        assert!(quota_url("http://bifrost.example.com").is_err());
        assert!(quota_url("http://127.0.0.1:8080").is_ok());
    }
}
