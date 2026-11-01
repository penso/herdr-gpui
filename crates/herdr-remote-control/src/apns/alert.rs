//! Notifications for the native app, sealed to the device
//! (`docs/ios-app-plan.md`, "Notifications, exactly"). APNs sees only a
//! cover ("Herdr", "New activity") and the chat's pane as the thread; the
//! app's Notification Service Extension opens `sealed` and shows what is
//! inside, with answer buttons for a permission request. The badge travels
//! sealed too: it is this computer's count, and a phone paired with several
//! shows their sum, so APNs must not set it alone.
//!
//! ```json
//! { "aps": { "alert": { "title": "Herdr", "body": "New activity" },
//!            "mutable-content": 1, "thread-id": "w1:p2",
//!            "interruption-level": "time-sensitive", "sound": "default" },
//!   "sealed": "BASE64URL" }
//! ```
//!
//! A clear (the question was answered elsewhere) is a background push,
//! `{"aps": {"content-available": 1}, "sealed": "…"}`, whose
//! sealed `clear` tells the app to remove the chat's notifications.

use super::{
    Error, Result,
    payload::MAX_PAYLOAD,
    send::{Priority, Push, PushType},
};
use crate::push::{Kind, Notification};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use herdr_chat_protocol::PermissionOption;
use serde::{Deserialize, Serialize};

/// The sealed format's version.
pub(crate) const VERSION: u32 = 1;
/// Answer buttons a notification offers at most.
pub(crate) const MAX_OPTIONS: usize = 4;
/// The longest `apns-collapse-id` APNs accepts, in bytes.
const MAX_COLLAPSE_ID: usize = 64;
/// What APNs and a phone whose extension cannot open the payload show.
const COVER_TITLE: &str = "Herdr";
const COVER_BODY: &str = "New activity";

/// What the app's extension opens. Field order is the wire order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sealed {
    pub(crate) v: u32,
    pub(crate) kind: Kind,
    /// Remove the chat's notifications instead of showing one.
    pub(crate) clear: bool,
    /// The computer's CA fingerprint, 64 uppercase hex digits without
    /// separators, which the app routes answers by.
    pub(crate) computer: String,
    pub(crate) pane: String,
    pub(crate) title: String,
    pub(crate) body: String,
    /// Questions waiting across every chat.
    pub(crate) badge: u32,
    /// The permission request a question is, for its answer buttons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) permission: Option<SealedPermission>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SealedPermission {
    pub(crate) id: u64,
    /// At most [`MAX_OPTIONS`], in the agent's order.
    pub(crate) options: Vec<PermissionOption>,
}

impl Sealed {
    /// The sealed side of `notification` from the computer whose CA
    /// fingerprint is `computer` (64 hex digits, no colons).
    pub(crate) fn new(notification: &Notification, computer: String, badge: u32) -> Self {
        Self {
            v: VERSION,
            kind: notification.kind,
            clear: notification.clear,
            computer,
            pane: notification.pane.clone(),
            title: notification.title.clone(),
            body: notification.body.clone(),
            badge,
            permission: notification
                .permission
                .as_ref()
                .map(|permission| SealedPermission {
                    id: permission.id.0,
                    options: permission
                        .options
                        .iter()
                        .take(MAX_OPTIONS)
                        .cloned()
                        .collect(),
                }),
        }
    }
}

#[derive(Serialize)]
struct Outer<'a> {
    aps: Aps<'a>,
    sealed: String,
}

#[derive(Serialize)]
struct Aps<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    alert: Option<Cover>,
    #[serde(rename = "mutable-content", skip_serializing_if = "Option::is_none")]
    mutable_content: Option<u8>,
    #[serde(rename = "content-available", skip_serializing_if = "Option::is_none")]
    content_available: Option<u8>,
    #[serde(rename = "thread-id", skip_serializing_if = "Option::is_none")]
    thread_id: Option<&'a str>,
    #[serde(rename = "interruption-level", skip_serializing_if = "Option::is_none")]
    interruption_level: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<&'static str>,
}

#[derive(Serialize)]
struct Cover {
    title: &'static str,
    body: &'static str,
}

/// The push for `sealed`, sealed by `seal`, under `tag` as its collapse
/// id. The body is shortened until the payload fits APNs's 4 KB.
pub(crate) fn push(
    mut sealed: Sealed,
    tag: &str,
    seal: impl Fn(&[u8]) -> Result<Vec<u8>>,
) -> Result<Push> {
    loop {
        let body = outer(&sealed, &seal)?;
        if body.len() <= MAX_PAYLOAD {
            let question = sealed.kind == Kind::Question && !sealed.clear;
            return Ok(Push {
                body,
                collapse_id: Some(collapse_id(tag)),
                priority: if question {
                    Priority::High
                } else {
                    Priority::Low
                },
                push_type: if sealed.clear {
                    PushType::Background
                } else {
                    PushType::Alert
                },
            });
        }
        if sealed.body.is_empty() {
            return Err(Error::PayloadTooLarge { len: body.len() });
        }
        // Base64 grows the sealed bytes by a third, so the plaintext must
        // lose three quarters of the excess.
        let excess = (body.len() - MAX_PAYLOAD).div_ceil(4) * 3;
        sealed.body = shorten(&sealed.body, excess);
    }
}

/// The outer JSON for `sealed`.
fn outer(sealed: &Sealed, seal: &impl Fn(&[u8]) -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    let plaintext = serde_json::to_vec(sealed)?;
    let aps = if sealed.clear {
        Aps {
            alert: None,
            mutable_content: None,
            content_available: Some(1),
            thread_id: None,
            interruption_level: None,
            sound: None,
        }
    } else {
        Aps {
            alert: Some(Cover {
                title: COVER_TITLE,
                body: COVER_BODY,
            }),
            mutable_content: Some(1),
            content_available: None,
            thread_id: Some(&sealed.pane),
            interruption_level: Some(if sealed.kind == Kind::Question {
                "time-sensitive"
            } else {
                "active"
            }),
            sound: Some("default"),
        }
    };
    Ok(serde_json::to_vec(&Outer {
        aps,
        sealed: URL_SAFE_NO_PAD.encode(seal(&plaintext)?),
    })?)
}

/// `text` at least `drop` bytes shorter, cut on a character boundary and
/// ending in `…`; empty when that leaves nothing.
fn shorten(text: &str, drop: usize) -> String {
    const ELLIPSIS: char = '\u{2026}';
    let base = text.strip_suffix(ELLIPSIS).unwrap_or(text);
    let Some(mut end) = base.len().checked_sub(drop + ELLIPSIS.len_utf8()) else {
        return String::new();
    };
    while !base.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        return String::new();
    }
    format!("{}{ELLIPSIS}", &base[..end])
}

/// `tag` cut to APNs's limit on a character boundary.
fn collapse_id(tag: &str) -> String {
    let mut end = tag.len().min(MAX_COLLAPSE_ID);
    while !tag.is_char_boundary(end) {
        end -= 1;
    }
    tag[..end].to_owned()
}
