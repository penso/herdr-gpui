//! `herdr-usage`'s typed values read as its own English did, now through
//! usage.ftl.
use super::super::text;
use herdr_usage::{
    Text,
    model::{Balance, Kind, Pace, Unit, Window},
};
use std::time::{Duration, SystemTime};

/// Drops Fluent's isolation marks around placeables.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

#[test]
fn countdowns_keep_the_coarsest_two_units() {
    let minutes = |m: u64| plain(text::countdown(Duration::from_secs(m * 60)));
    assert_eq!(minutes(47), "47m");
    assert_eq!(minutes(2 * 60 + 53), "2h 53m");
    assert_eq!(minutes(4 * 1440 + 11 * 60), "4d 11h");
    assert_eq!(
        plain(text::updated(Duration::from_secs(30))),
        "Updated less than a minute ago"
    );
    assert_eq!(
        plain(text::updated(Duration::from_secs(5 * 60 + 30))),
        "Updated 5m ago"
    );
}

#[test]
fn pace_reads_as_the_crate_describes_it() {
    let pace = |expected, runs_out| Pace { expected, runs_out };
    for (pace, used) in [
        (pace(50., None), 39.),
        (pace(50., None), 50.2),
        (pace(30., Some(Duration::from_secs(3600))), 60.),
        (pace(30., None), 100.),
    ] {
        assert_eq!(plain(text::pace(&pace, used)), pace.describe(used));
    }
}

#[test]
fn limits_name_their_kind_or_the_service_name() {
    let now = SystemTime::now();
    let weekly = Window::new(Kind::Weekly, 11., None, None);
    assert_eq!(plain(text::left_heading(&weekly)), "Weekly 89% left");
    assert_eq!(plain(text::bar_label(&weekly, now)), weekly.label(now));
    let named = Window::new(Kind::Named(Text::service("Opus")), 3., None, None);
    assert_eq!(plain(text::title(&named.kind)), "Opus");
    assert_eq!(plain(text::bar_label(&named, now)), named.label(now));
    let session = Window::new(
        Kind::Session,
        3.,
        Some(now + Duration::from_secs(3 * 3600)),
        None,
    );
    assert_eq!(plain(text::bar_label(&session, now)), session.label(now));
}

#[test]
fn balances_read_their_total() {
    let dollars = Balance::new(Text::service("Credits"), 12.3, Unit::Currency("USD".into()));
    assert_eq!(plain(text::balance(&dollars)), dollars.text());
    let budget = dollars.out_of(50.);
    assert_eq!(plain(text::balance(&budget)), budget.text());
}

#[test]
fn other_accounts_read_as_claude_swap_notes_them() {
    use herdr_usage::details::{ClaudeAccount, ClaudeAccountStatus as Status};
    for status in [
        Status::Ok,
        Status::TokenExpired,
        Status::ApiKey,
        Status::KeychainUnavailable,
        Status::ReloginRequired,
        Status::ForeignCredential,
        Status::NoCredentials,
        Status::Unavailable,
        Status::Unknown,
    ] {
        for (stale, disabled) in [(false, false), (true, false), (false, true), (true, true)] {
            let account = ClaudeAccount {
                email: "one@example.com".into(),
                alias: None,
                disabled,
                status,
                stale,
                windows: Vec::new(),
            };
            let english = account
                .note()
                .into_iter()
                .chain(disabled.then_some("disabled"))
                .collect::<Vec<_>>()
                .join(", ");
            assert_eq!(plain(super::super::details::note(&account)), english);
        }
    }
}
