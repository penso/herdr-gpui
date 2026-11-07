use super::*;
use crate::credential::LIFETIME_S;

#[test]
fn a_credential_says_what_it_was_issued_for() {
    let text = issuer().issue(PHONE, Host::Sandbox, NOW_S);
    let credential = issuer().check(&text).unwrap();
    assert_eq!(credential.device, PHONE);
    assert_eq!(credential.environment, Host::Sandbox);
    assert_eq!(credential.expires_s, NOW_S + LIFETIME_S);
}

#[test]
fn another_secret_or_a_changed_byte_is_refused() {
    let text = issuer().issue(PHONE, Host::Production, NOW_S);
    assert!(Issuer::new(&[6; 32]).unwrap().check(&text).is_none());

    let bytes = URL_SAFE_NO_PAD.decode(&text).unwrap();
    for at in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[at] ^= 1;
        let changed = URL_SAFE_NO_PAD.encode(changed);
        assert!(issuer().check(&changed).is_none(), "{at}");
    }
}

#[test]
fn malformed_text_is_refused() {
    let text = issuer().issue(PHONE, Host::Production, NOW_S);
    for bad in ["", "not base64!", "AAAA", &text[..text.len() - 4]] {
        assert!(issuer().check(bad).is_none(), "{bad}");
    }
}
