#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::push::Chat;

mod alert;
mod decide;
mod notify_key;
mod pacer;
mod payload;
mod redact;
mod tokens;

const CHAT: Chat<'static> = Chat {
    agent: "Claude",
    pane: "w1-p3",
    title: "remote plan",
    workspace: "herdr-gpui",
};

fn token(byte: u8) -> DeviceToken {
    DeviceToken::parse(&format!("{byte:02x}").repeat(32)).unwrap()
}
