//! Deepgram usage, read from the Management API with an API key sent as
//! `Authorization: Token …`: the `api_key` setting (or `DEEPGRAM_API_KEY`
//! here), else `DEEPGRAM_API_KEY` on the probed host. Without a configured
//! project, usage is summed over every project the key can see, as CodexBar
//! does. Deepgram reports usage, not a balance or a limit, so the panel
//! shows facts only. Everything CodexBar reads is ported.

use crate::text::Text;
use crate::{
    Result,
    {
        model::{Account, Provider, Report, Section, group},
        probe::{Probe, Request, Secret},
        service::{Meta, Service, Setting, json},
        values::https_base,
    },
};
use serde::Deserialize;

const BASE: &str = "https://api.deepgram.com/v1";
/// A key that sees many projects costs one request each; this bounds a refresh.
const MAX_PROJECTS: usize = 20;

pub(crate) struct Deepgram;

static META: Meta = Meta::new("deepgram", "Deepgram")
    .dashboard("https://console.deepgram.com/project/")
    .settings(&[
        Setting::new(
            "api_key",
            &["DEEPGRAM_API_KEY"],
            "usage-deepgram-setting-api-key",
        ),
        Setting::new(
            "project_id",
            &["DEEPGRAM_PROJECT_ID"],
            "usage-deepgram-setting-project-id",
        ),
        Setting::new(
            "base_url",
            &["DEEPGRAM_API_URL"],
            "usage-deepgram-setting-base-url",
        ),
    ]);

impl Service for Deepgram {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let key = probe
            .setting("api_key")
            .or_else(|| probe.env("DEEPGRAM_API_KEY"))?;
        Some(fetch(probe, &key))
    }
}

fn fetch(probe: &mut Probe, key: &Secret) -> Result<Report> {
    // An override must stay on HTTPS: the key is attached to it.
    let base = https_base(probe.text_setting("base_url"), BASE)?;
    let get = |url: String| Request::get(url).secret_header("Authorization", "Token ", key);
    let projects = match probe.text_setting("project_id").filter(|id| !id.is_empty()) {
        Some(id) => vec![Project {
            project_id: id,
            name: None,
        }],
        None => parse_projects(&probe.body(get(format!("{base}/projects")))?)?,
    };
    let mut totals = Totals::default();
    for project in projects.iter().take(MAX_PROJECTS) {
        let id: String =
            url::form_urlencoded::byte_serialize(project.project_id.as_bytes()).collect();
        let body = probe.body(get(format!("{base}/projects/{id}/usage/breakdown")))?;
        totals.add(parse_breakdown(&body)?);
    }
    Ok(report(&projects, &totals))
}

fn parse_projects(body: &str) -> Result<Vec<Project>> {
    Ok(json::<Projects>(body)?.projects)
}

fn parse_breakdown(body: &str) -> Result<Totals> {
    let breakdown: Breakdown = json(body)?;
    let mut totals = Totals {
        start: breakdown.start,
        end: breakdown.end,
        ..Totals::default()
    };
    for row in breakdown.results {
        totals.hours += row.hours.unwrap_or(0.);
        totals.total_hours += row.total_hours.unwrap_or(0.);
        totals.agent_hours += row.agent_hours.unwrap_or(0.);
        totals.tokens_in += row.tokens_in.unwrap_or(0.);
        totals.tokens_out += row.tokens_out.unwrap_or(0.);
        totals.tts += row.tts_characters.unwrap_or(0.);
        totals.requests += row.requests.unwrap_or(0.);
    }
    Ok(totals)
}

fn report(projects: &[Project], totals: &Totals) -> Report {
    let plan = match projects {
        [] => Text::message("usage-deepgram-no-projects"),
        [project] => Text::message("usage-deepgram-project").arg(
            "project",
            project.name.as_deref().unwrap_or(&project.project_id),
        ),
        projects => Text::message("usage-deepgram-projects").arg("count", projects.len()),
    };
    let mut facts = vec![(
        Text::message("usage-deepgram-requests"),
        Text::service(whole(totals.requests)),
    )];
    if totals.hours > 0. || totals.total_hours > 0. {
        facts.push((
            Text::message("usage-deepgram-audio"),
            Text::message("usage-deepgram-audio-hours")
                .arg("hours", decimal(totals.hours))
                .arg("billable", decimal(totals.total_hours)),
        ));
    }
    if totals.agent_hours > 0. {
        facts.push((
            Text::message("usage-deepgram-agent-hours"),
            Text::service(decimal(totals.agent_hours)),
        ));
    }
    if totals.tokens_in > 0. || totals.tokens_out > 0. {
        facts.push((
            Text::message("usage-deepgram-tokens"),
            Text::service(whole(totals.tokens_in + totals.tokens_out)),
        ));
    }
    if totals.tts > 0. {
        facts.push((
            Text::message("usage-deepgram-tts-characters"),
            Text::service(whole(totals.tts)),
        ));
    }
    if let (Some(start), Some(end)) = (&totals.start, &totals.end) {
        facts.push((
            Text::message("usage-deepgram-period"),
            Text::message("usage-deepgram-period-range")
                .arg("start", start.as_str())
                .arg("end", end.as_str()),
        ));
    }
    Report::new(
        Provider(&Deepgram),
        Account {
            email: None,
            plan: Some(plan),
        },
        Vec::new(),
    )
    .with_sections([Section::Facts {
        title: Text::message("usage-deepgram-usage-summary"),
        facts,
    }])
}

fn whole(value: f64) -> String {
    group(value.round() as i64)
}

fn decimal(value: f64) -> String {
    if value.fract() == 0. {
        whole(value)
    } else {
        format!("{value:.1}")
    }
}

#[derive(Deserialize)]
struct Projects {
    projects: Vec<Project>,
}

#[derive(Deserialize)]
struct Project {
    project_id: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct Breakdown {
    start: Option<String>,
    end: Option<String>,
    results: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    hours: Option<f64>,
    total_hours: Option<f64>,
    agent_hours: Option<f64>,
    tokens_in: Option<f64>,
    tokens_out: Option<f64>,
    tts_characters: Option<f64>,
    requests: Option<f64>,
}

#[derive(Default)]
struct Totals {
    start: Option<String>,
    end: Option<String>,
    hours: f64,
    total_hours: f64,
    agent_hours: f64,
    tokens_in: f64,
    tokens_out: f64,
    tts: f64,
    requests: f64,
}

impl Totals {
    /// Sums another project's usage, widening the period to cover both.
    fn add(&mut self, other: Self) {
        self.hours += other.hours;
        self.total_hours += other.total_hours;
        self.agent_hours += other.agent_hours;
        self.tokens_in += other.tokens_in;
        self.tokens_out += other.tokens_out;
        self.tts += other.tts;
        self.requests += other.requests;
        if let Some(start) = other.start
            && self.start.as_ref().is_none_or(|current| start < *current)
        {
            self.start = Some(start);
        }
        if let Some(end) = other.end
            && self.end.as_ref().is_none_or(|current| end > *current)
        {
            self.end = Some(end);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const PROJECTS: &str = r#"{"projects":[
        {"project_id":"3f1c2a9e-1111-4b8e-9d7e-000000000001","name":"Voice bot"},
        {"project_id":"3f1c2a9e-1111-4b8e-9d7e-000000000002","name":null}
    ]}"#;

    const BREAKDOWN: &str = r#"{
        "start":"2026-06-01","end":"2026-06-30",
        "resolution":{"units":"day","amount":1},
        "results":[
            {"start":"2026-06-01","end":"2026-06-15","hours":1.5,"total_hours":2,
             "agent_hours":0.5,"tokens_in":1000,"tokens_out":250,"tts_characters":1200,"requests":40},
            {"start":"2026-06-16","end":"2026-06-30","hours":0.3,"total_hours":0.5,
             "requests":10,"tokens_in":null}
        ]
    }"#;

    #[test]
    fn sums_usage_across_projects() {
        let projects = parse_projects(PROJECTS).unwrap();
        assert_eq!(projects.len(), 2);
        let mut totals = Totals::default();
        totals.add(parse_breakdown(BREAKDOWN).unwrap());
        totals
            .add(parse_breakdown(r#"{"start":"2026-05-20","results":[{"requests":5}]}"#).unwrap());
        let report = report(&projects, &totals);
        assert!(report.windows.is_empty());
        assert_eq!(
            report.account.plan,
            Some(Text::message("usage-deepgram-projects").arg("count", 2))
        );
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        let fact = |label: &str| {
            facts
                .iter()
                .find(|(name, _)| name == label)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(fact("usage-deepgram-requests"), Some(Text::service("55")));
        assert_eq!(
            fact("usage-deepgram-audio"),
            Some(
                Text::message("usage-deepgram-audio-hours")
                    .arg("hours", "1.8")
                    .arg("billable", "2.5")
            )
        );
        assert_eq!(fact("usage-deepgram-tokens"), Some(Text::service("1,250")));
        assert_eq!(
            fact("usage-deepgram-tts-characters"),
            Some(Text::service("1,200"))
        );
        assert_eq!(
            fact("usage-deepgram-period"),
            Some(
                Text::message("usage-deepgram-period-range")
                    .arg("start", "2026-05-20")
                    .arg("end", "2026-06-30")
            )
        );
    }

    #[test]
    fn names_a_single_project() {
        let projects = parse_projects(PROJECTS).unwrap();
        let report = report(&projects[..1], &parse_breakdown(BREAKDOWN).unwrap());
        assert_eq!(
            report.account.plan,
            Some(Text::message("usage-deepgram-project").arg("project", "Voice bot"))
        );
    }

    #[test]
    fn rejects_plain_http_overrides() {
        assert!(https_base(Some("http://proxy.test/v1".into()), BASE).is_err());
        assert_eq!(
            https_base(Some("proxy.test/v1/".into()), BASE).unwrap(),
            "https://proxy.test/v1"
        );
        assert_eq!(https_base(None, BASE).unwrap(), BASE);
    }

    #[test]
    fn rejects_a_breakdown_without_results() {
        assert!(parse_breakdown(r#"{"start":"2026-06-01"}"#).is_err());
    }
}
