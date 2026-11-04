use super::*;
use crate::apns::jwt::{REFRESH_S, sign};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, signature::Verifier as _};

const NOW: u64 = 1_800_000_000;

fn decode(part: &str) -> serde_json::Value {
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
}

#[test]
fn token_has_the_key_id_team_and_issue_time_and_verifies() {
    let token = sign(&key(), TEAM, KEY_ID, NOW).unwrap();
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);

    let header = decode(parts[0]);
    assert_eq!(header, serde_json::json!({"alg": "ES256", "kid": KEY_ID}));
    let claims = decode(parts[1]);
    assert_eq!(claims, serde_json::json!({"iat": NOW, "iss": TEAM}));

    let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    assert_eq!(signature.len(), 64, "ES256 is r || s, not DER");
    let signature = Signature::from_slice(&signature).unwrap();
    let signing_input = token.rsplit_once('.').unwrap().0;
    key()
        .verifying_key()
        .verify(signing_input.as_bytes(), &signature)
        .unwrap();
}

#[test]
fn token_is_reused_for_fifty_minutes_then_signed_again() {
    let provider = ProviderToken::new(key(), TEAM.to_owned(), KEY_ID.to_owned());
    let first = provider.current(NOW).unwrap();
    assert_eq!(provider.current(NOW + REFRESH_S - 1).unwrap(), first);
    let second = provider.current(NOW + REFRESH_S).unwrap();
    assert_ne!(second, first);
    assert_eq!(
        decode(second.split('.').nth(1).unwrap())["iat"],
        NOW + REFRESH_S
    );
}

#[test]
fn invalidated_token_is_signed_again() {
    let provider = ProviderToken::new(key(), TEAM.to_owned(), KEY_ID.to_owned());
    let first = provider.current(NOW).unwrap();
    provider.invalidate();
    let second = provider.current(NOW + 1).unwrap();
    assert_ne!(second, first);
}

#[test]
fn a_clock_that_went_back_signs_again() {
    let provider = ProviderToken::new(key(), TEAM.to_owned(), KEY_ID.to_owned());
    let first = provider.current(NOW).unwrap();
    let earlier = provider.current(NOW - 10).unwrap();
    assert_ne!(earlier, first);
    assert_eq!(decode(earlier.split('.').nth(1).unwrap())["iat"], NOW - 10);
}
