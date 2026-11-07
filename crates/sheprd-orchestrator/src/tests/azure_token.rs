//! After a refusal the shell's personal access token goes last: it is read
//! once per run, so it may be the token just refused.
use crate::azure::token::{self, Found, Source, Token};
use std::ffi::OsString;

fn pat(_: &str) -> Option<OsString> {
    Some(OsString::from("from-the-shell"))
}

fn entra() -> Option<Token> {
    Some(Token::Entra("from-the-cli".into()))
}

fn source(found: Option<Found>) -> Option<Source> {
    found.map(|found| found.source)
}

#[test]
fn a_refused_personal_token_gives_way_to_the_cli_sign_in() {
    assert_eq!(
        source(token::renewed(|| None, entra, pat)),
        Some(Source::Cli)
    );
}

#[test]
fn without_a_cli_sign_in_the_personal_token_stays() {
    assert_eq!(
        source(token::renewed(|| None, || None, pat)),
        Some(Source::Environment)
    );
    assert!(token::renewed(|| None, || None, |_| None).is_none());
}

#[test]
fn a_token_saved_since_the_refusal_takes_over_from_the_shell() {
    assert_eq!(
        source(token::renewed(|| Some("pasted".into()), entra, pat)),
        Some(Source::Saved)
    );
}
