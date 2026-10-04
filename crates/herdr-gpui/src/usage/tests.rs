#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use super::probe::{HostPath, Request, Shell};
use super::{
    Host, Message, Reading, Usage, UsageConfig,
    cookies::{self, CookieJar},
    model::{
        Account, Balance, Kind, Provider, Report, SESSION, Section, Severity, Unit, WEEK, Window,
        countdown, group,
    },
    probe::{Exec, Probe, Response, json_field},
    providers::{claude, codex},
    registry,
    settings::ProviderSettings,
};
use crate::Error;
use std::time::{Duration, Instant, SystemTime};

mod browser_cookies;
mod labels;
mod provider_data;
mod refresh;
mod remote_hosts;
mod status_bar;

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn provider(id: &str) -> Provider {
    registry::find(id).unwrap()
}

fn report(provider: Provider, used: f64) -> Report {
    Report::new(
        provider,
        Account::default(),
        vec![Window::new(Kind::Session, used, None, None)],
    )
}
