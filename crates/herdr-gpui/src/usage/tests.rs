#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{
    Reading,
    model::{Account, Kind, Provider, Report, Window},
    registry,
};

mod en_xa;
mod example_config;
mod footer_fit;
mod icons;
mod keychain_grants;
mod report_messages;
mod section;
mod status_bar_room;
mod text;
mod words;

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
