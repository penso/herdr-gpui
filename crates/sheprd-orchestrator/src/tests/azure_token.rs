//! After a refusal the Azure CLI's sign-in goes first: the shell's personal
//! access token is read once per run, so it may be the token just refused.
use crate::azure::token::{self, Token};
use std::ffi::OsString;

fn pat(_: &str) -> Option<OsString> {
    Some(OsString::from("from-the-shell"))
}

fn entra() -> Option<Token> {
    Some(Token::Entra("from-the-cli".into()))
}

#[test]
fn a_refused_personal_token_gives_way_to_the_cli_sign_in() {
    assert!(matches!(token::renewed(entra, pat), Some(Token::Entra(_))));
}

#[test]
fn without_a_cli_sign_in_the_personal_token_stays() {
    assert!(matches!(
        token::renewed(|| None, pat),
        Some(Token::Personal(_))
    ));
    assert!(token::renewed(|| None, |_| None).is_none());
}
