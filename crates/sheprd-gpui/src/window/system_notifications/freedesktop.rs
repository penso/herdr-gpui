//! Freedesktop notifications over one shared session-bus connection. GPUI's
//! Linux platform opens a connection per notification and keeps it until the
//! notification reports a close, which a daemon that closes it at once (do
//! not disturb, history-only delivery) never lets it see, so each post leaked
//! one bus peer until the bus refused new ones (#387).
//!
//! The click rule is registered before the first `Notify`, so a click on even
//! the first notification is seen. Closes are not tracked: the bounded
//! registry and coalesced outbox are the only per-notification state.

mod outbox;
pub(super) use outbox::{Outbox, post};

#[cfg(test)]
mod tests;

use super::Registry;
use futures_lite::StreamExt;
use gpui::{App, SystemNotification};
use oo7::zbus::{self, MatchRule, Message, MessageStream, message::Type, zvariant::Value};
use std::{borrow::Cow, collections::HashMap};

const NAME: &str = "org.freedesktop.Notifications"; // i18n: data
const PATH: &str = "/org/freedesktop/Notifications"; // i18n: data
/// The action a click on the notification body invokes.
const DEFAULT_ACTION: &str = "default";
/// Clicks waiting for the UI thread; older ones are dropped, not queued.
const QUEUED_CLICKS: usize = 16;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("the notification service refused the request")]
    Bus(#[from] zbus::Error),
}

/// How notifications reach the desktop.
#[derive(Default)]
pub(crate) enum Bus {
    /// GPUI posts. Only launches that never connect use it: tests, whose
    /// platform holds no bus.
    #[default]
    Gpui,
    /// Connecting, or the session bus could not be reached: nothing posts.
    Unavailable,
    Connected(Session),
}

/// The shared connection and what the notification server does with it.
#[derive(Clone)]
pub(crate) struct Session {
    connection: zbus::Connection,
    /// Whether the server reads bodies as markup, where `<a href>` and
    /// `<img src>` would let terminal text reach links and local files.
    markup: bool,
}

/// Opens the shared connection and routes clicks for as long as it lives.
pub(super) fn connect(cx: &mut App) {
    cx.global_mut::<Registry>().bus = Bus::Unavailable;
    cx.spawn(async move |cx| {
        let mut clicks = match listen().await {
            Ok((session, clicks)) => {
                cx.update(|cx| cx.global_mut::<Registry>().bus = Bus::Connected(session));
                clicks
            }
            Err(error) => {
                tracing::warn!(error = ?error, "System notifications are off: no session bus");
                return;
            }
        };
        while let Some(message) = clicks.next().await {
            let Some(id) = message.ok().as_ref().and_then(clicked) else {
                continue;
            };
            cx.update(|cx| {
                let tag = cx
                    .global::<Registry>()
                    .find_bus_id(id)
                    .map(|p| p.tag.clone());
                if let Some(tag) = tag {
                    super::open(&tag, cx);
                }
            });
        }
    })
    .detach();
}

async fn listen() -> Result<(Session, MessageStream), Error> {
    let connection = zbus::Connection::session().await?;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .path(PATH)?
        .interface(NAME)?
        .member("ActionInvoked")? // i18n: data
        .build();
    // Returns once the bus has added the rule.
    let clicks = MessageStream::for_match_rule(rule, &connection, Some(QUEUED_CLICKS)).await?;
    let markup = markup(&connection).await;
    Ok((Session { connection, markup }, clicks))
}

/// Assumes markup when the server will not say, so text is escaped rather
/// than interpreted.
async fn markup(connection: &zbus::Connection) -> bool {
    let reply = connection
        .call_method(Some(NAME), PATH, Some(NAME), "GetCapabilities", &()) // i18n: data
        .await;
    match reply.and_then(|reply| reply.body().deserialize::<Vec<String>>()) {
        Ok(capabilities) => capabilities.iter().any(|c| c == "body-markup"),
        Err(error) => {
            tracing::debug!(error = ?error, "Escaping notification bodies: no capabilities");
            true
        }
    }
}

/// The body as plain text for a server that reads it as markup.
fn plain(body: &str, markup: bool) -> Cow<'_, str> {
    if !markup || !body.contains(['&', '<', '>']) {
        return Cow::Borrowed(body);
    }
    Cow::Owned(
        body.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;"),
    )
}

/// The daemon's id of the notification whose body was clicked.
fn clicked(message: &Message) -> Option<u32> {
    let (id, action) = message.body().deserialize::<(u32, String)>().ok()?;
    (action == DEFAULT_ACTION).then_some(id)
}

/// Posts `notification`, replacing the daemon's `replaces` (0 for none), and
/// returns the daemon's id for it.
pub(super) async fn show(
    session: &Session,
    notification: &SystemNotification,
    replaces: u32,
) -> Result<u32, Error> {
    // Silent: sounds stay with the sound service.
    let hints = HashMap::from([("suppress-sound", Value::from(true))]);
    let body = plain(&notification.body, session.markup);
    let reply = session
        .connection
        .call_method(
            Some(NAME),
            PATH,
            Some(NAME),
            "Notify", // i18n: data
            &(
                crate::constants::WINDOW_TITLE,
                replaces,
                "",
                // The spec keeps the summary plain text.
                notification.title.as_ref(),
                body.as_ref(),
                [DEFAULT_ACTION, DEFAULT_ACTION].as_slice(),
                hints,
                -1_i32,
            ),
        )
        .await?;
    Ok(reply.body().deserialize::<u32>()?)
}
