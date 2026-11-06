#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    credential::{DeviceId, Issuer},
    handle::{Keyring, key_id},
    limit::{Rate, Rates},
    relay::{Gateway, PushRequest, Relay},
    store::Store,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::SecretKey;
use serde_json::json;
use sheprd_apns::{DeviceToken, Host, Outcome, Priority, Push, PushType, seal};
use std::{
    collections::VecDeque,
    net::IpAddr,
    sync::{Arc, Mutex},
};
use tempfile::TempDir;

mod credential;
mod handle;
mod limit;
mod options;
mod relay;
mod serve;
mod store;
mod web;

const SECRET: &[u8] = &[5; 32];
/// Noon on a day in 2026, in seconds.
const NOW_S: u64 = 1_790_000_000;
const NOW_MS: u64 = NOW_S * 1000;
const PHONE: DeviceId = [1; 16];
const OTHER_PHONE: DeviceId = [2; 16];
const PAYLOAD: &str = r#"{"aps":{"content-available":1},"sealed":"bm90aGluZyB0byBzZWU"}"#;

fn relay_key() -> SecretKey {
    SecretKey::from_slice(&[8; 32]).unwrap()
}

fn issuer() -> Issuer {
    Issuer::new(SECRET).unwrap()
}

fn source() -> IpAddr {
    "203.0.113.9".parse().unwrap()
}

fn token_hex(byte: u8) -> String {
    format!("{byte:02x}").repeat(32)
}

fn token(byte: u8) -> DeviceToken {
    DeviceToken::parse(&token_hex(byte)).unwrap()
}

/// Seals `opened` as the phone does, under `info`, to `key`.
fn seal_to(key: &SecretKey, opened: &serde_json::Value, info: &[u8]) -> String {
    let bytes = serde_json::to_vec(opened).unwrap();
    let sealed = seal::seal(&key.public_key(), &bytes, info).unwrap();
    format!("{}.{}", key_id(key), URL_SAFE_NO_PAD.encode(sealed))
}

/// A handle as a phone makes it, with its token from `byte`.
fn handle_with(credential: &str, kind: &str, id: &str, byte: u8) -> String {
    seal_to(
        &relay_key(),
        &json!({ "credential": credential, "id": id, "kind": kind, "token": token_hex(byte) }),
        seal::HANDLE_INFO,
    )
}

/// A good handle for `device` of `kind`.
fn handle(device: DeviceId, kind: &str, id_byte: u8) -> String {
    let credential = issuer().issue(device, Host::Production, NOW_S);
    let id = format!("{id_byte:02x}").repeat(16);
    handle_with(&credential, kind, &id, device[0])
}

fn request(handle: String, push_type: PushType) -> PushRequest {
    PushRequest {
        collapse_id: None,
        handle,
        payload: PAYLOAD.to_owned(),
        priority: Priority::High,
        push_type,
    }
}

/// Stands in for APNs: records pushes, and answers from a script, then with
/// success.
#[derive(Clone, Default)]
struct Apple {
    answers: Arc<Mutex<VecDeque<sheprd_apns::Result<Outcome>>>>,
    sent: Arc<Mutex<Vec<(DeviceToken, Push)>>>,
}

impl Apple {
    fn answer(&self, answer: sheprd_apns::Result<Outcome>) {
        self.answers.lock().unwrap().push_back(answer);
    }

    fn sent(&self) -> Vec<(DeviceToken, Push)> {
        self.sent.lock().unwrap().clone()
    }
}

impl Gateway for Apple {
    async fn post(
        &self,
        token: &DeviceToken,
        push: &Push,
        _now_s: u64,
    ) -> sheprd_apns::Result<Outcome> {
        self.sent
            .lock()
            .unwrap()
            .push((token.clone(), push.clone()));
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(Outcome::Delivered))
    }
}

/// A relay on a mock APNs with a database in a temporary directory.
struct Fixture {
    apple: Apple,
    dir: TempDir,
    relay: Relay<Apple>,
    store: Store,
}

fn fixture_with(rates: Rates) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("relay.sqlite")).unwrap();
    let apple = Apple::default();
    let relay = Relay::new(
        apple.clone(),
        Host::Production,
        issuer(),
        Keyring::new(relay_key(), None),
        store.clone(),
        rates,
    );
    Fixture {
        apple,
        dir,
        relay,
        store,
    }
}

fn fixture() -> Fixture {
    fixture_with(Rates::default())
}

/// A limit of `burst` that does not refill within a test.
fn tight(burst: u32) -> Rate {
    Rate {
        burst,
        per_minute: 0,
    }
}
