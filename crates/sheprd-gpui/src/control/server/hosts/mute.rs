//! `host mute` and `host unmute`: a device's mute, which stays in this app.
use super::{Place, named};
use crate::{
    SheprdWindow, control::protocol::Caller, control::protocol::Response, notification_mutes::Mutes,
};
use gpui::Context;

/// Mutes or unmutes the host named by ID or label, else the caller's.
pub(super) fn set(
    view: &mut SheprdWindow,
    caller: &Caller,
    host: Option<&str>,
    muted: bool,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    let found = match host {
        Some(name) => {
            named(view, name).map(|endpoint| (endpoint.id.clone(), endpoint.label.clone()))
        }
        None => Place::of(view, caller).map(|place| (place.endpoint, place.label)),
    };
    let (id, label) = match found {
        Ok(found) => found,
        Err(response) => return response,
    };
    view.set_host_muted(&id, muted, cx);
    if Mutes::of(cx).is_some_and(|mutes| mutes.host(&id)) != muted {
        return Response::error(
            crate::control::protocol::ErrorCode::InvalidRequest,
            format!("The ID of host {label} is too long to keep a mute on"),
        );
    }
    Response::done(if muted {
        format!("Muted {label}")
    } else {
        format!("Unmuted {label}")
    })
}
