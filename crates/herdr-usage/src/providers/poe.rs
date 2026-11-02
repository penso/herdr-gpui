//! Poe point balance and recent spend, read from Poe's usage API with an API
//! key: the `api_key` setting (or `POE_API_KEY` here), else `POE_API_KEY` on
//! the probed host. The balance is required; the last 30 days of points
//! history are best-effort, as in CodexBar, so a history failure never hides
//! the balance. CodexBar's daily bar chart is summarized as per-period facts
//! and a per-model split. Poe has no OAuth or cookie sign-in to port.

use crate::model::Count;
use crate::text::Text;
use crate::{
    Result,
    {
        model::{Account, Balance, DAY, Provider, Report, Section, Unit, group},
        probe::{Probe, Request, Secret},
        service::{Meta, Service, Setting, Timestamp, json, number},
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

const BALANCE: &str = "https://api.poe.com/usage/current_balance";
const HISTORY: &str = "https://api.poe.com/usage/points_history";
/// CodexBar reads at most five pages of history per refresh.
const MAX_PAGES: usize = 5;
const PERIOD: Duration = Duration::from_secs(30 * 86_400);

pub(crate) struct Poe;

static META: Meta = Meta::new("poe", "Poe")
    .dashboard("https://poe.com/api/keys")
    .settings(&[Setting::new(
        "api_key",
        &["POE_API_KEY"],
        "usage-poe-setting-api-key",
    )]);

impl Service for Poe {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let key = probe
            .setting("api_key")
            .or_else(|| probe.env("POE_API_KEY"))?;
        Some(fetch(probe, &key))
    }
}

fn fetch(probe: &mut Probe, key: &Secret) -> Result<Report> {
    let body = probe.body(Request::get(BALANCE).bearer(key))?;
    let balance = parse_balance(&body)?;
    let now = SystemTime::now();
    let cutoff = now.checked_sub(PERIOD).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut entries = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let query = match &cursor {
            Some(cursor) => format!(
                "?limit=100&starting_after={}",
                url::form_urlencoded::byte_serialize(cursor.as_bytes()).collect::<String>()
            ),
            None => "?limit=100".to_owned(),
        };
        let page = probe
            .body(Request::get(format!("{HISTORY}{query}")).bearer(key))
            .and_then(|body| parse_history(&body, cutoff));
        let Ok(page) = page else {
            break;
        };
        entries.extend(page.entries);
        match page.next {
            Some(next) if !page.past_cutoff => cursor = Some(next),
            _ => break,
        }
    }
    Ok(report(balance, &entries, now))
}

fn parse_balance(body: &str) -> Result<Option<f64>> {
    #[derive(Deserialize)]
    struct Current {
        #[serde(default, deserialize_with = "number")]
        current_point_balance: Option<f64>,
    }
    Ok(json::<Current>(body)?.current_point_balance)
}

struct Entry {
    at: SystemTime,
    points: f64,
    cost: Option<f64>,
    model: String,
}

struct Page {
    entries: Vec<Entry>,
    next: Option<String>,
    /// The page's last row is older than the cutoff, so later pages are too.
    past_cutoff: bool,
}

fn parse_history(body: &str, cutoff: SystemTime) -> Result<Page> {
    let root: serde_json::Map<String, Value> = json(body)?;
    let rows = ["data", "items", "results"]
        .into_iter()
        .find_map(|key| root.get(key).and_then(Value::as_array))
        .map(Vec::as_slice)
        .unwrap_or_default();
    let entries = rows
        .iter()
        .filter_map(|row| {
            let at = row_time(row)?;
            if at < cutoff {
                return None;
            }
            let points = ["cost_points", "points", "point_cost"]
                .into_iter()
                .find_map(|key| loose_number(row.get(key)?))
                .unwrap_or(0.)
                .max(0.);
            let cost = ["cost_usd", "usd"]
                .into_iter()
                .find_map(|key| loose_number(row.get(key)?));
            let model = row
                .get("bot_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or("unknown")
                .to_owned();
            Some(Entry {
                at,
                points,
                cost,
                model,
            })
        })
        .collect();
    let last = rows.last();
    let next = root
        .get("next_cursor")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|next| !next.is_empty())
        .or_else(|| {
            (root.get("has_more") == Some(&Value::Bool(true)))
                .then(|| last?.get("query_id")?.as_str())
                .flatten()
                .map(str::trim)
                .filter(|id| !id.is_empty())
        })
        .map(str::to_owned);
    Ok(Page {
        entries,
        next,
        past_cutoff: last.and_then(row_time).is_some_and(|at| at < cutoff),
    })
}

fn row_time(row: &Value) -> Option<SystemTime> {
    let value = ["creation_time", "timestamp", "created_at"]
        .into_iter()
        .find_map(|key| row.get(key).filter(|value| !value.is_null()))?;
    // Poe's creation_time is in microseconds; seconds and milliseconds are
    // accepted as CodexBar does.
    if let Some(number) = loose_number(value) {
        if !(number.is_finite() && number > 0.) {
            return None;
        }
        let seconds = if number > 1e14 {
            number / 1e6
        } else if number > 1e12 {
            number / 1e3
        } else {
            number
        };
        return SystemTime::UNIX_EPOCH.checked_add(Duration::try_from_secs_f64(seconds).ok()?);
    }
    Timestamp::Text(value.as_str()?.to_owned()).time()
}

fn loose_number(value: &Value) -> Option<f64> {
    let number: f64 = match value {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

#[derive(Default)]
struct Summary {
    points: f64,
    requests: u64,
    cost: Option<f64>,
}

impl Summary {
    fn of<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Self {
        entries.into_iter().fold(Self::default(), |mut sum, entry| {
            sum.points += entry.points;
            sum.requests += 1;
            if let Some(cost) = entry.cost {
                sum.cost = Some(sum.cost.unwrap_or(0.) + cost.max(0.));
            }
            sum
        })
    }

    fn text(&self) -> Text {
        match self.cost {
            None => Text::message("usage-poe-summary")
                .arg("points", points(self.points))
                .arg("requests", self.requests)
                .arg("count", group(self.requests as i64)),
            Some(cost) => Text::message("usage-poe-summary-cost")
                .arg("points", points(self.points))
                .arg("requests", self.requests)
                .arg("count", group(self.requests as i64))
                .arg("cost", format!("${cost:.2}")),
        }
    }
}

fn report(balance: Option<f64>, entries: &[Entry], now: SystemTime) -> Report {
    let day = |at: SystemTime| {
        at.duration_since(SystemTime::UNIX_EPOCH)
            .map(|since| since.as_secs() / DAY.as_secs())
            .unwrap_or(0)
    };
    let today = day(now);
    let week = now.checked_sub(7 * DAY).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut facts = Vec::new();
    if let Some(balance) = balance {
        facts.push((Text::message("usage-poe-current-balance"), points(balance)));
    }
    let mut sections = Vec::new();
    if !entries.is_empty() {
        let summaries = [
            (
                Text::message("usage-poe-today"),
                Summary::of(entries.iter().filter(|entry| day(entry.at) == today)),
            ),
            (
                Text::message("usage-poe-last-7-days"),
                Summary::of(entries.iter().filter(|entry| entry.at >= week)),
            ),
            (
                Text::message("usage-poe-last-30-days"),
                Summary::of(entries),
            ),
        ];
        facts.extend(
            summaries
                .into_iter()
                .map(|(label, summary)| (label, summary.text())),
        );
        let mut models: HashMap<&str, f64> = HashMap::new();
        for entry in entries {
            *models.entry(&entry.model).or_default() += entry.points;
        }
        let mut models: Vec<_> = models.into_iter().collect();
        models.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        if let Some((model, spent)) = models.first() {
            facts.push((
                Text::message("usage-poe-top-model"),
                Text::message("usage-poe-model-points")
                    .arg("model", *model)
                    .arg("points", points(*spent)),
            ));
        }
        let total: f64 = models.iter().map(|(_, spent)| spent).sum();
        if total > 0. {
            sections.push(Section::Shares {
                title: Text::message("usage-poe-points-by-model"),
                shares: models
                    .iter()
                    .take(5)
                    .map(|(model, spent)| (Text::service(*model), (spent / total * 100.) as f32))
                    .collect(),
            });
        }
    }
    let facts = (!facts.is_empty()).then(|| Section::Facts {
        title: Text::message("usage-poe-points"),
        facts,
    });
    Report::new(Provider(&Poe), Account::default(), Vec::new())
        .with_balances(balance.map(|balance| {
            Balance::new(
                Text::message("usage-poe-points"),
                balance,
                Unit::Count(Count::Points),
            )
        }))
        .with_sections(facts.into_iter().chain(sections))
}

fn points(value: f64) -> Text {
    let amount = if value >= 1000. || value.fract() == 0. {
        group(value.round() as i64)
    } else {
        format!("{value:.1}")
    };
    Text::message("usage-poe-point-count")
        .arg("points", value)
        .arg("amount", amount)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn point_count(points: f64, amount: &str) -> Text {
        Text::message("usage-poe-point-count")
            .arg("points", points)
            .arg("amount", amount)
    }

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn reads_numeric_and_string_balances() {
        assert_eq!(
            parse_balance(r#"{"current_point_balance":1500}"#).unwrap(),
            Some(1500.)
        );
        assert_eq!(
            parse_balance(r#"{"current_point_balance":"2500"}"#).unwrap(),
            Some(2500.)
        );
        assert_eq!(parse_balance("{}").unwrap(), None);
    }

    #[test]
    fn balance_alone_is_a_report() {
        let report = report(Some(1500.), &[], at(1_800_000_000));
        assert!(report.windows.is_empty());
        assert_eq!(report.balances[0].text(), "1,500 points");
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        assert_eq!(
            facts,
            &[(
                Text::message("usage-poe-current-balance"),
                Text::message("usage-poe-point-count")
                    .arg("points", 1500.)
                    .arg("amount", "1,500")
            )]
        );
    }

    #[test]
    fn summarizes_history_by_period_and_model() {
        let now = at(1_800_000_000);
        let cutoff = now - PERIOD;
        // Microsecond creation times, as the points history reports them.
        let body = format!(
            r#"{{"data":[
                {{"query_id":"q3","bot_name":"Claude-Sonnet","creation_time":{today},"usage_type":"API","cost_points":300,"cost_usd":"0.09"}},
                {{"query_id":"q2","bot_name":"GPT-5","creation_time":{three_days},"usage_type":"Chat","cost_points":100}},
                {{"query_id":"q1","bot_name":"Claude-Sonnet","creation_time":{twenty_days},"cost_points":"600"}},
                {{"query_id":"q0","bot_name":"Old","creation_time":{old},"cost_points":999}}
            ],"has_more":true}}"#,
            today = (1_800_000_000u64 - 60) * 1_000_000,
            three_days = (1_800_000_000u64 - 3 * 86_400) * 1_000_000,
            twenty_days = (1_800_000_000u64 - 20 * 86_400) * 1_000_000,
            old = (1_800_000_000u64 - 40 * 86_400) * 1_000_000,
        );
        let page = parse_history(&body, cutoff).unwrap();
        assert_eq!(page.entries.len(), 3);
        assert_eq!(page.next.as_deref(), Some("q0"));
        assert!(page.past_cutoff);

        let report = report(Some(42.5), &page.entries, now);
        assert_eq!(report.balances[0].amount, 42.5);
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        let summary = |points: f64, amount: &str, requests: u64| {
            Text::message("usage-poe-summary-cost")
                .arg("points", point_count(points, amount))
                .arg("requests", requests)
                .arg("count", requests.to_string())
                .arg("cost", "$0.09")
        };
        assert_eq!(
            facts,
            &[
                (
                    Text::message("usage-poe-current-balance"),
                    point_count(42.5, "42.5")
                ),
                (Text::message("usage-poe-today"), summary(300., "300", 1)),
                (
                    Text::message("usage-poe-last-7-days"),
                    summary(400., "400", 2)
                ),
                (
                    Text::message("usage-poe-last-30-days"),
                    summary(1000., "1,000", 3)
                ),
                (
                    Text::message("usage-poe-top-model"),
                    Text::message("usage-poe-model-points")
                        .arg("model", "Claude-Sonnet")
                        .arg("points", point_count(900., "900"))
                ),
            ]
        );
        let Section::Shares { shares, .. } = &report.sections[1] else {
            panic!("expected shares");
        };
        assert_eq!(shares[0], (Text::service("Claude-Sonnet"), 90.));
        assert_eq!(shares[1], (Text::service("GPT-5"), 10.));
    }

    #[test]
    fn follows_next_cursor() {
        let page = parse_history(r#"{"items":[],"next_cursor":" abc "}"#, at(0)).unwrap();
        assert_eq!(page.next.as_deref(), Some("abc"));
        assert!(!page.past_cutoff);
    }
}
