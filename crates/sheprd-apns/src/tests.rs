#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use p256::{SecretKey, ecdsa::SigningKey};

mod config;
mod jwt;
mod seal;
mod send;
mod token;

const TEAM: &str = "ABCDE12345";
const KEY_ID: &str = "XYZ9876543";
const TOPIC: &str = "so.pen.herdr.remote";

/// A fixed P-256 key, so tests need no randomness.
fn key() -> SigningKey {
    SigningKey::from(SecretKey::from_slice(&[7; 32]).unwrap())
}

fn config(sandbox: bool) -> Config {
    Config::from_parts(TEAM, KEY_ID, TOPIC, sandbox, key()).unwrap()
}

fn token(byte: u8) -> DeviceToken {
    DeviceToken::parse(&format!("{byte:02x}").repeat(32)).unwrap()
}
