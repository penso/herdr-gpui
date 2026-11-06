//! The rule that keeps a stolen disk useless: no token, handle, or payload
//! reaches the database or the logs, on any path.
//!
//! This is its own test binary with one test, on purpose. `tracing` caches
//! whether each log line is enabled, so with tests on other threads and a
//! subscriber on this one, a line can be cached as off and a leak go unseen.
//! Only a global subscriber, set before any line runs, sees them all.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::SecretKey;
use serde_json::json;
use sheprd_apns::{DeviceToken, Host, Outcome, Priority, Push, PushType, seal};
use sheprd_push_relay::{
    credential::Issuer,
    handle::{Keyring, key_id},
    limit::Rates,
    relay::{Gateway, PushRequest, Relay, Reply, RevokeRequest},
    store::Store,
};
use std::{
    collections::VecDeque,
    fs, io,
    net::IpAddr,
    sync::{Arc, Mutex},
};
use tracing_subscriber::fmt::MakeWriter;

const NOW_S: u64 = 1_790_000_000;
const SECRET_TEXT: &str = "the-payload-must-not-be-kept";

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl MakeWriter<'_> for Captured {
    type Writer = Self;

    fn make_writer(&self) -> Self {
        self.clone()
    }
}

/// Answers from a script.
#[derive(Clone, Default)]
struct Apple(Arc<Mutex<VecDeque<sheprd_apns::Result<Outcome>>>>);

impl Gateway for Apple {
    async fn post(
        &self,
        _token: &DeviceToken,
        _push: &Push,
        _now_s: u64,
    ) -> sheprd_apns::Result<Outcome> {
        self.0.lock().unwrap().pop_front().unwrap()
    }
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[tokio::test]
async fn nothing_of_a_push_reaches_the_disk_or_the_logs() {
    let logs = Captured::default();
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(logs.clone())
        .init();

    let secret = [5; 32];
    let key = SecretKey::from_slice(&[8; 32]).unwrap();
    let token = "01".repeat(32);
    let credential = Issuer::new(&secret)
        .unwrap()
        .issue([1; 16], Host::Production, NOW_S);
    let opened = json!({
        "credential": credential, "id": "a".repeat(32), "kind": "alert", "token": token,
    });
    let sealed = seal::seal(
        &key.public_key(),
        &serde_json::to_vec(&opened).unwrap(),
        seal::HANDLE_INFO,
    )
    .unwrap();
    let sealed = URL_SAFE_NO_PAD.encode(sealed);
    let handle = format!("{}.{sealed}", key_id(&key));

    let dir = tempfile::tempdir().unwrap();
    let apple = Apple::default();
    let answers = [
        Ok(Outcome::Delivered),
        Ok(Outcome::Gone),
        Ok(Outcome::Rejected {
            reason: Some("BadCollapseId".to_owned()),
            status: 400,
        }),
        Err(sheprd_apns::Error::Timeout),
    ];
    let count = answers.len();
    apple.0.lock().unwrap().extend(answers);
    let relay = Relay::new(
        apple.clone(),
        Host::Production,
        Issuer::new(&secret).unwrap(),
        Keyring::new(key, None),
        Store::open(dir.path().join("relay.sqlite")).unwrap(),
        Rates::default(),
    );

    let source: IpAddr = "203.0.113.9".parse().unwrap();
    for _ in 0..count {
        let push = PushRequest {
            collapse_id: None,
            handle: handle.clone(),
            payload: format!(r#"{{"sealed":"{SECRET_TEXT}"}}"#),
            priority: Priority::High,
            push_type: PushType::Alert,
        };
        relay.push(source, push, NOW_S * 1000).await;
    }
    let revoke = RevokeRequest {
        handle: handle.clone(),
    };
    assert_eq!(
        relay.revoke(source, revoke, NOW_S * 1000).await,
        Reply::Revoked
    );

    assert!(
        apple.0.lock().unwrap().is_empty(),
        "every push went through"
    );
    let logged = logs.0.lock().unwrap().clone();
    assert!(contains(&logged, "BadCollapseId"), "the capture works");
    for needle in [token.as_str(), SECRET_TEXT, sealed.as_str()] {
        assert!(!contains(&logged, needle), "the logs hold {needle}");
        for file in fs::read_dir(dir.path()).unwrap() {
            let file = file.unwrap();
            let bytes = fs::read(file.path()).unwrap();
            assert!(
                !contains(&bytes, needle),
                "{:?} holds {needle}",
                file.path()
            );
        }
    }
}
