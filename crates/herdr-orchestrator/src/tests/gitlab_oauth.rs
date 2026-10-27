//! Connecting a GitLab account with OAuth: GitLab's device and token replies,
//! the record line that keeps the pair, renewal only when due, and
//! which application connects which instance.
use crate::gitlab::{
    self,
    oauth::{GITLAB_COM_APPLICATION, Poll, parse_device, parse_token},
    saved::{Kept, OAuth, Saved},
};
use secrecy::{ExposeSecret, SecretString};

fn secret(text: &str) -> SecretString {
    SecretString::from(text)
}

fn oauth(access: &str, expires_at: u64) -> OAuth {
    OAuth {
        access: secret(access),
        refresh: secret("refresh-1"),
        expires_at,
        application: "app-id".into(),
    }
}

#[test]
fn a_device_code_opens_only_an_https_page_on_its_instance() {
    let reply = |complete: &str| {
        format!(
            r#"{{"device_code":"dev-1","user_code":"ABCD-EFGH","verification_uri":"https://gitlab.com/oauth/device","verification_uri_complete":"{complete}","expires_in":300,"interval":5}}"#
        )
    };
    let device = parse_device(
        200,
        reply("https://gitlab.com/oauth/device?user_code=ABCD-EFGH").as_bytes(),
        "gitlab.com",
    )
    .unwrap();
    assert_eq!(device.user_code, "ABCD-EFGH");
    assert_eq!(
        device.open.as_str(),
        "https://gitlab.com/oauth/device?user_code=ABCD-EFGH"
    );
    assert_eq!(device.interval.as_secs(), 5);
    // A page elsewhere, or over HTTP, falls back to the plain page here.
    for elsewhere in [
        "https://evil.example/device",
        "http://gitlab.com/oauth/device",
    ] {
        let device = parse_device(200, reply(elsewhere).as_bytes(), "gitlab.com").unwrap();
        assert_eq!(device.open.as_str(), "https://gitlab.com/oauth/device");
    }
    assert!(matches!(
        parse_device(200, reply("x").as_bytes(), "gitlab.example.com"),
        Err(gitlab::Error::Address(_))
    ));
    assert!(matches!(
        parse_device(401, br#"{"error":"invalid_client"}"#, "gitlab.com"),
        Err(gitlab::Error::Authentication)
    ));
}

#[test]
fn a_token_reply_grants_a_pair_or_says_why_not() {
    let granted = parse_token(
        200,
        br#"{"access_token":"access-1","refresh_token":"refresh-1","expires_in":7200,"token_type":"Bearer"}"#,
        "app-id",
        1_000,
    )
    .unwrap();
    let Poll::Granted(pair) = granted else {
        panic!("not granted");
    };
    assert_eq!(pair.access.expose_secret(), "access-1");
    assert_eq!(pair.refresh.expose_secret(), "refresh-1");
    assert_eq!(pair.expires_at, 8_200);
    assert_eq!(pair.application, "app-id");
    let poll = |body: &[u8]| parse_token(400, body, "app-id", 0);
    assert!(matches!(
        poll(br#"{"error":"authorization_pending"}"#),
        Ok(Poll::Pending)
    ));
    assert!(matches!(
        poll(br#"{"error":"slow_down"}"#),
        Ok(Poll::SlowDown)
    ));
    assert!(matches!(
        poll(br#"{"error":"access_denied"}"#),
        Err(gitlab::Error::Denied)
    ));
    assert!(matches!(
        poll(br#"{"error":"expired_token"}"#),
        Err(gitlab::Error::CodeExpired)
    ));
    assert!(matches!(
        poll(br#"{"error":"invalid_grant"}"#),
        Err(gitlab::Error::Authentication)
    ));
    // A pair without a refresh token cannot be kept.
    assert!(matches!(
        parse_token(200, br#"{"access_token":"access-1"}"#, "app-id", 0),
        Err(gitlab::Error::Token)
    ));
}

#[test]
fn an_oauth_sign_in_is_kept_beside_pasted_tokens() {
    let mut saved = Saved::default();
    saved
        .set("gitlab.example.com", Some(secret("glpat-x")))
        .unwrap();
    saved
        .set_kept("gitlab.com", Some(Kept::OAuth(oauth("access-1", 42))))
        .unwrap();
    let record =
        secret("gitlab.com\toauth\taccess-1\trefresh-1\t42\tapp-id\ngitlab.example.com\tglpat-x\n");
    let read = Saved::parse(&record).unwrap();
    let Some(Kept::OAuth(kept)) = read.kept("gitlab.com") else {
        panic!("not an OAuth sign-in");
    };
    assert_eq!(kept.expires_at, 42);
    assert_eq!(read.get("gitlab.com").unwrap().expose_secret(), "access-1");
    assert!(matches!(
        read.kept("gitlab.example.com"),
        Some(Kept::Token(_))
    ));
    for malformed in [
        "gitlab.com\toauth\taccess-1\trefresh-1\tsoon\tapp-id\n",
        "gitlab.com\toauth\taccess-1\trefresh-1\t42\tapp id\n",
        "gitlab.com\toauth\taccess-1\n",
    ] {
        assert!(
            matches!(Saved::parse(&secret(malformed)), Err(gitlab::Error::Record)),
            "{malformed:?}"
        );
    }
}

#[test]
fn gitlab_dot_com_connects_with_herdrs_application_and_others_with_their_own() {
    let mut config = gitlab::Config::default();
    assert_eq!(
        config.application("gitlab.com"),
        Some(GITLAB_COM_APPLICATION)
    );
    assert_eq!(config.application("gitlab.example.com"), None);
    config
        .applications
        .insert("GitLab.Example.com".into(), "self-managed".into());
    config
        .applications
        .insert("gitlab.com".into(), "own".into());
    assert_eq!(
        config.application("gitlab.example.com"),
        Some("self-managed")
    );
    assert_eq!(config.application("gitlab.com"), Some("own"));
}

#[cfg(unix)]
#[test]
fn an_oauth_token_is_renewed_and_kept_only_when_due() {
    use crate::gitlab::saved::{save_kept_in, token_in};
    use herdr_credentials::Store;
    use std::cell::Cell;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    save_kept_in(
        Store::File,
        path,
        "gitlab.com",
        Some(Kept::OAuth(oauth("old", 10_000))),
    )
    .unwrap();
    let renewals = Cell::new(0);
    let renew = |_: &OAuth, now: u64| {
        renewals.set(renewals.get() + 1);
        Ok(oauth("new", now + 7_200))
    };
    // An hour left: used as is.
    let token = token_in(Store::File, path, "gitlab.com", 10_000 - 3_600, renew).unwrap();
    assert_eq!(token.unwrap().expose_secret(), "old");
    assert_eq!(renewals.get(), 0);
    // Five minutes left: renewed, and the new pair kept before it is used.
    let token = token_in(Store::File, path, "gitlab.com", 10_000 - 300, renew).unwrap();
    assert_eq!(token.unwrap().expose_secret(), "new");
    assert_eq!(renewals.get(), 1);
    let kept = gitlab::saved::read_in(Store::File, path).unwrap();
    assert_eq!(kept.get("gitlab.com").unwrap().expose_secret(), "new");
    // A renewal that fails keeps the old token for GitLab to refuse.
    save_kept_in(
        Store::File,
        path,
        "gitlab.com",
        Some(Kept::OAuth(oauth("stale", 0))),
    )
    .unwrap();
    let token = token_in(Store::File, path, "gitlab.com", 100, |_, _| {
        Err(gitlab::Error::Authentication)
    })
    .unwrap();
    assert_eq!(token.unwrap().expose_secret(), "stale");
    assert!(
        token_in(Store::File, path, "gitlab.example.com", 0, renew)
            .unwrap()
            .is_none()
    );
}

#[test]
fn the_largest_valid_record_fits_every_store() {
    use crate::gitlab::saved::MAX_HOSTS;
    let mut saved = Saved::default();
    let long = |fill: char| SecretString::from(fill.to_string().repeat(1024));
    for index in 0..MAX_HOSTS {
        let host = format!("{index:02}{}", "h".repeat(250));
        let oauth = OAuth {
            access: long('a'),
            refresh: long('r'),
            expires_at: u64::MAX,
            application: "x".repeat(256),
        };
        saved.set_kept(&host, Some(Kept::OAuth(oauth))).unwrap();
    }
    let record = saved.record();
    assert_eq!(Saved::parse(&record).unwrap().hosts().count(), MAX_HOSTS);
    assert!(record.expose_secret().len() <= herdr_credentials::LIMIT);
}
