//! The words a report names, written by the app: messages with their
//! arguments read in English as the provider used to write them, and counts
//! take their unit's plural.
use super::super::text;
use herdr_usage::{
    Text,
    model::{Balance, Count, Unit},
};

/// Drops Fluent's isolation marks around placeables.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn english(words: Text) -> String {
    plain(text::words(&words))
}

#[test]
fn the_services_words_stay_as_they_are() {
    assert_eq!(english(Text::service("Pro · Team")), "Pro · Team");
}

#[test]
fn wayfinder_reads_as_it_did() {
    let state = |status: &str, models: usize, mode: &str| {
        english(
            Text::message("usage-wayfinder-gateway-state")
                .arg("status", status)
                .arg("models", models)
                .arg("mode", mode),
        )
    };
    assert_eq!(state("degraded", 2, "live"), "degraded · 2 models");
    assert_eq!(state("ok", 1, "both"), "ok · 1 model · offline · dry run");
    assert_eq!(state("ok", 3, "offline"), "ok · 3 models · offline");
    assert_eq!(state("ok", 3, "dry-run"), "ok · 3 models · dry run");
    let degraded =
        |missing: usize| english(Text::message("usage-wayfinder-degraded").arg("missing", missing));
    assert_eq!(degraded(0), "Degraded");
    assert_eq!(degraded(1), "Degraded, 1 key missing");
    assert_eq!(degraded(3), "Degraded, 3 keys missing");
    assert_eq!(
        english(Text::message("usage-wayfinder-account").arg("models", 2)),
        "2 models · local gateway"
    );
    assert_eq!(
        english(
            Text::message("usage-wayfinder-saved-value")
                .arg("amount", "$4.12")
                .arg("percent", "73.3")
        ),
        "$4.12 · 73.3% vs highest-cost route"
    );
    assert_eq!(
        english(Text::message("usage-wayfinder-milliseconds").arg("time", "0.2")),
        "0.2 ms"
    );
}

#[test]
fn counts_take_their_units_plural() {
    let amount = |amount: f64, count: Count| {
        plain(text::balance(&Balance::new(
            Text::service(""),
            amount,
            Unit::Count(count),
        )))
    };
    assert_eq!(amount(1., Count::Credits), "1 credit");
    assert_eq!(amount(1_250., Count::Credits), "1,250 credits");
    assert_eq!(amount(1., Count::Points), "1 point");
    assert_eq!(amount(2.5, Count::Points), "2.50 points");
    assert_eq!(amount(25., Count::Reviews), "25 reviews");
    assert_eq!(amount(12., Count::Named("DIEM".into())), "12 DIEM");
    // Every plural reads as the crate's own English does.
    for count in [Count::Credits, Count::Points, Count::Reviews] {
        let balance = Balance::new(Text::service(""), 1_250_000., Unit::Count(count.clone()));
        assert_eq!(plain(text::balance(&balance)), balance.amount_text());
    }
}
