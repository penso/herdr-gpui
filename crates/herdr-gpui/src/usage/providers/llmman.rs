//! llmman model memory, read from a running `llmman serve` daemon's own node
//! report (`/llmman/node`, plus `/api/version` best effort), as CodexBar
//! does. No inference is run.
//!
//! The daemon is looked for at the configured `base_url`, else at the probed
//! host's `LLMMAN_HOST`, else at `http://127.0.0.1:17434` on the probed host,
//! normalized as CodexBar and llmman do. A daemon started with `LLMMAN_API_KEYS` needs a key: the
//! host's `LLMMAN_API_KEY`, else the config's `api_key`. When nothing is
//! configured and no daemon answers, the provider is not detected.
//!
//! A key from this machine's config makes the request run from this
//! machine, so on a remote host it is only used with a non-loopback address
//! from this machine's config, where it cannot reach this machine's own
//! daemon instead, nor be sent where the remote host's `LLMMAN_HOST` points.
//! `LLMMAN_HOST` is read on the probed host, which is this app's environment
//! when the host is this machine, as in CodexBar.

use crate::{
    Error, Result,
    usage::{
        model::{Account, Kind, Provider, Report, Section, Window},
        probe::{Probe, Request, Secret},
        service::{Meta, Service, Setting, json},
    },
};
use serde::Deserialize;
use std::{collections::BTreeMap, net::IpAddr, time::Duration};
use url::{Host, Url};

const DEFAULT: &str = "http://127.0.0.1:17434";
const PORT: u16 = 17434;
const TIMEOUT: Duration = Duration::from_secs(5);
/// Loaded models listed in the panel, largest first.
const MODELS: usize = 24;

pub(crate) struct Llmman;

static META: Meta = Meta::new("llmman", "llmman").settings(&[
    Setting::new(
        "base_url",
        &[],
        "The llmman serve address, when not http://127.0.0.1:17434. A host without a \
             scheme uses HTTP on port 17434, e.g. localhost:18000. Plain HTTP is allowed \
             only for loopback, private-network, and .local hosts; others need HTTPS. \
             Without it, LLMMAN_HOST on the probed host is used when set.",
    ),
    Setting::new(
        "api_key",
        &["LLMMAN_API_KEY"],
        "Only when the daemon was started with LLMMAN_API_KEYS: one of those keys.",
    ),
]);

impl Service for Llmman {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let (address, source) = match probe.text_setting("base_url") {
            Some(address) => (Some(address), Source::Config),
            None => (probe.env_text("LLMMAN_HOST"), Source::Host),
        };
        let Some(daemon) = daemon(address.as_deref()) else {
            return Some(Err(Error::UsageConnect));
        };
        let host_key = probe.env("LLMMAN_API_KEY");
        let key = host_key.or_else(|| {
            local_key_allowed(probe.is_remote(), source, &daemon)
                .then(|| probe.setting("api_key"))
                .flatten()
        });
        let configured = address.is_some() || key.is_some();

        let response = match probe.http(daemon.request("/llmman/node", key.as_ref())) {
            Ok(response) => response,
            Err(_) if !configured => return None,
            Err(error) => return Some(Err(error)),
        };
        match response.status {
            401 | 403 if key.is_none() => return Some(Err(Error::UsageNotSignedIn)),
            200..=299 => {}
            // Nothing, or another server, answers at the default address.
            _ if !configured => return None,
            _ => {}
        }
        let body = match response.ok() {
            Ok(body) => body,
            Err(error) => return Some(Err(error)),
        };

        let version = probe
            .http(daemon.request("/api/version", key.as_ref()))
            .ok()
            .and_then(|reply| reply.json::<Version>().ok())
            .and_then(|reply| reply.version)
            .filter(|version| label(version));
        Some(parse(&body, key.is_some(), version))
    }
}

/// Where the daemon address came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    /// This machine's config.
    Config,
    /// The probed host's `LLMMAN_HOST`, or the default address.
    Host,
}

/// Whether this machine's `api_key` may go with a request to `daemon`. On a
/// remote host the request runs from this machine, so the key only goes to a
/// non-loopback address this machine's config chose.
fn local_key_allowed(remote: bool, source: Source, daemon: &Daemon) -> bool {
    !remote || (source == Source::Config && !daemon.loopback)
}

struct Daemon {
    base: String,
    loopback: bool,
}

impl Daemon {
    fn request(&self, path: &str, key: Option<&Secret>) -> Request {
        let request = Request::get(format!("{}{path}", self.base)).timeout(TIMEOUT);
        match key {
            Some(key) => request.bearer(key),
            None => request,
        }
    }
}

/// The daemon origin, or None when the address is not a safe place to send
/// a key: a scheme-less host is HTTP on port 17434, `0.0.0.0` means this
/// host, HTTP is limited to private networks, and credentials, queries, and
/// fragments are refused. An OpenAI-style `/v1` suffix is dropped.
fn daemon(address: Option<&str>) -> Option<Daemon> {
    let Some(address) = address.map(str::trim).filter(|address| !address.is_empty()) else {
        return Some(Daemon {
            base: DEFAULT.to_owned(),
            loopback: true,
        });
    };
    let explicit = address.contains("://");
    let mut url = if explicit {
        Url::parse(address)
    } else {
        Url::parse(&format!("http://{address}"))
    }
    .ok()?;
    if url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.scheme(), "http" | "https")
    {
        return None;
    }
    if url.host_str() == Some("0.0.0.0") {
        url.set_host(Some("127.0.0.1")).ok()?;
    }
    if !explicit && url.port().is_none() {
        url.set_port(Some(PORT)).ok()?;
    }
    let (private, loopback) = match url.host()? {
        Host::Domain(domain) => {
            let domain = domain.to_ascii_lowercase();
            let loopback = domain == "localhost" || domain.ends_with(".localhost");
            (loopback || domain.ends_with(".local"), loopback)
        }
        Host::Ipv4(ip) => private_ip(IpAddr::V4(ip)),
        Host::Ipv6(ip) => private_ip(IpAddr::V6(ip)),
    };
    if url.scheme() == "http" && !private {
        return None;
    }
    let base = url.as_str().trim_end_matches('/');
    let base = base.strip_suffix("/v1").unwrap_or(base).to_owned();
    Some(Daemon { base, loopback })
}

/// Whether an address is private, and whether it is this host.
fn private_ip(ip: IpAddr) -> (bool, bool) {
    match ip {
        IpAddr::V4(ip) => (
            ip.is_loopback() || ip.is_private() || ip.is_link_local(),
            ip.is_loopback(),
        ),
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            let unique_local = first & 0xfe00 == 0xfc00;
            let link_local = first & 0xffc0 == 0xfe80;
            (
                ip.is_loopback() || unique_local || link_local,
                ip.is_loopback(),
            )
        }
    }
}

/// A model name or version worth showing: short and printable.
fn label(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= 120 && !text.chars().any(char::is_control)
}

pub(crate) fn parse(body: &str, keyed: bool, version: Option<String>) -> Result<Report> {
    let node: Node = json(body)?;
    let total = |models: &BTreeMap<String, u64>| models.values().sum::<u64>();
    let in_use = total(&node.loaded);
    let count =
        |models: &BTreeMap<String, u64>| format!("{} · {}", models.len(), size(total(models)));

    let windows = if node.memory > 0 {
        vec![Window::new(
            Kind::Named("Memory".into()),
            in_use as f64 / node.memory as f64 * 100.,
            None,
            None,
        )]
    } else {
        Vec::new()
    };

    let mut summary = vec![
        ("Loaded".to_owned(), count(&node.loaded)),
        ("Stored".to_owned(), count(&node.stored)),
    ];
    if node.memory > 0 {
        summary.push((
            "Memory".to_owned(),
            format!("{} of {}", size(in_use), size(node.memory)),
        ));
    }
    summary.extend(version.map(|version| ("Version".to_owned(), version)));

    let mut loaded: Vec<_> = node.loaded.iter().filter(|(name, _)| label(name)).collect();
    loaded.sort_by(|(a, a_size), (b, b_size)| b_size.cmp(a_size).then_with(|| a.cmp(b)));
    let models: Vec<_> = loaded
        .into_iter()
        .take(MODELS)
        .map(|(name, weight)| (name.clone(), size(*weight)))
        .collect();

    let plan = if keyed { "API key" } else { "Local daemon" };
    let mut sections = vec![Section::Facts {
        title: "Daemon".into(),
        facts: summary,
    }];
    if !models.is_empty() {
        sections.push(Section::Facts {
            title: "Loaded models".into(),
            facts: models,
        });
    }
    Ok(Report::new(
        Provider(&Llmman),
        Account {
            email: None,
            plan: Some(plan.to_owned()),
        },
        windows,
    )
    .with_sections(sections))
}

/// Decimal units, as `llmman list` and `llmman ps` print sizes.
fn size(bytes: u64) -> String {
    let (unit, scale) = match bytes {
        0..1_000 => return format!("{bytes} B"),
        1_000..1_000_000 => ("kB", 1e3),
        1_000_000..1_000_000_000 => ("MB", 1e6),
        _ => ("GB", 1e9),
    };
    format!("{:.1} {unit}", bytes as f64 / scale)
}

#[derive(Deserialize)]
struct Node {
    memory: u64,
    loaded: BTreeMap<String, u64>,
    stored: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
struct Version {
    version: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// CodexBar's fixture: 8 GB + 2 GB loaded of 40 GB model memory.
    const NODE: &str = r#"{"memory":40000000000,
        "loaded":{"fixture/small:Q4_K_M":2000000000,"fixture/large:Q4_K_M":8000000000},
        "stored":{"fixture/small:Q4_K_M":2000000000,"fixture/large:Q4_K_M":8000000000,"fixture/idle":500000000}}"#;

    fn facts(report: &Report, index: usize) -> (&str, Vec<(&str, &str)>) {
        match &report.sections[index] {
            Section::Facts { title, facts } => (
                title.as_str(),
                facts
                    .iter()
                    .map(|(label, value)| (label.as_str(), value.as_str()))
                    .collect(),
            ),
            section => panic!("unexpected section {section:?}"),
        }
    }

    #[test]
    fn loaded_weights_fill_the_memory_window_largest_first() {
        let report = parse(NODE, true, Some("0.9.0".into())).unwrap();
        assert_eq!(report.windows.len(), 1);
        assert_eq!(report.windows[0].kind, Kind::Named("Memory".into()));
        assert_eq!(report.windows[0].percent(), 25);
        assert_eq!(report.windows[0].resets_at, None);
        assert_eq!(report.account.plan.as_deref(), Some("API key"));
        assert_eq!(
            facts(&report, 0),
            (
                "Daemon",
                vec![
                    ("Loaded", "2 · 10.0 GB"),
                    ("Stored", "3 · 10.5 GB"),
                    ("Memory", "10.0 GB of 40.0 GB"),
                    ("Version", "0.9.0"),
                ]
            )
        );
        assert_eq!(
            facts(&report, 1),
            (
                "Loaded models",
                vec![
                    ("fixture/large:Q4_K_M", "8.0 GB"),
                    ("fixture/small:Q4_K_M", "2.0 GB"),
                ]
            )
        );
    }

    #[test]
    fn an_idle_open_daemon_still_reports() {
        let report = parse(r#"{"memory":0,"loaded":{},"stored":{}}"#, false, None).unwrap();
        assert!(report.windows.is_empty());
        assert_eq!(report.account.plan.as_deref(), Some("Local daemon"));
        assert_eq!(report.sections.len(), 1);
        assert_eq!(
            facts(&report, 0).1,
            [("Loaded", "0 · 0 B"), ("Stored", "0 · 0 B")]
        );
    }

    #[test]
    fn malformed_node_reports_are_rejected() {
        for body in [
            "not json",
            "[]",
            r#"{"memory":-1,"loaded":{},"stored":{}}"#,
            r#"{"memory":1,"loaded":[],"stored":{}}"#,
            r#"{"memory":1,"loaded":{"a":"1"},"stored":{}}"#,
            r#"{"memory":1,"loaded":{}}"#,
        ] {
            assert!(
                matches!(parse(body, false, None), Err(Error::UsageJson(_))),
                "accepted {body}"
            );
        }
    }

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(size(999), "999 B");
        assert_eq!(size(1_500), "1.5 kB");
        assert_eq!(size(2_000_000), "2.0 MB");
        assert_eq!(size(10_500_000_000), "10.5 GB");
    }

    #[test]
    fn addresses_normalize_as_llmman_does() {
        for (address, base, loopback) in [
            ("localhost", "http://localhost:17434", true),
            ("127.0.0.1", "http://127.0.0.1:17434", true),
            ("0.0.0.0", "http://127.0.0.1:17434", true),
            ("0.0.0.0:18000", "http://127.0.0.1:18000", true),
            ("http://0.0.0.0:18000", "http://127.0.0.1:18000", true),
            ("192.168.1.10", "http://192.168.1.10:17434", false),
            ("daemon.local", "http://daemon.local:17434", false),
            ("[::1]", "http://[::1]:17434", true),
            ("127.0.0.1:18000", "http://127.0.0.1:18000", true),
            ("http://localhost", "http://localhost", true),
            (
                "https://llmman.example.com",
                "https://llmman.example.com",
                false,
            ),
            ("http://127.0.0.1:17434///", "http://127.0.0.1:17434", true),
            ("http://127.0.0.1:17434/v1/", "http://127.0.0.1:17434", true),
        ] {
            let daemon = daemon(Some(address)).expect(address);
            assert_eq!(daemon.base, base, "{address}");
            assert_eq!(daemon.loopback, loopback, "{address}");
        }
        let default = daemon(None).unwrap();
        assert_eq!(default.base, DEFAULT);
        assert!(default.loopback);
        assert_eq!(daemon(Some("  ")).unwrap().base, DEFAULT);
    }

    #[test]
    fn a_local_key_follows_only_a_configured_remote_address() {
        let lan = daemon(Some("192.168.1.20")).unwrap();
        let local = daemon(None).unwrap();
        assert!(local_key_allowed(false, Source::Host, &local));
        assert!(local_key_allowed(false, Source::Host, &lan));
        assert!(local_key_allowed(true, Source::Config, &lan));
        assert!(!local_key_allowed(true, Source::Config, &local));
        assert!(!local_key_allowed(true, Source::Host, &lan));
        assert!(!local_key_allowed(true, Source::Host, &local));
    }

    #[test]
    fn unsafe_addresses_are_refused() {
        for address in [
            "http://127.0.0.1:17434?probe=1",
            "http://127.0.0.1:17434#x",
            "localhost?probe=1",
            "127.0.0.1#x",
            "http://127.0.0.1:17434?",
            "http://127.0.0.1:17434#",
            "http://public.example.com",
            "public.example.com",
            "http://user:password@127.0.0.1:17434",
            "https://user:password@example.com",
            "file:///tmp/llmman",
        ] {
            assert!(daemon(Some(address)).is_none(), "accepted {address}");
        }
    }
}
