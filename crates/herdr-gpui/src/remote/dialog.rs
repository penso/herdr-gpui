//! The pairing dialog in a window: the daemon's pairing link as a QR with a
//! countdown ring, the addresses and certificate fingerprint under it, then
//! the approval step with the phone's name and matching code. The requests
//! run off the UI thread; [`super::pairing`] decides what each answer means.

use super::{
    pairing::{Answer, Pairing, Phase, spaced},
    qr::{self, Qr},
    supervisor::{Client, Lease, Supervisor},
};
use crate::{HerdrWindow, config::Theme, fonts::StyledFont, menu::Page};
use gpui::{prelude::*, *};
use herdr_remote_control::control::Request;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// How often the dialog looks for a request to send, and redraws its ring.
const STEP: Duration = Duration::from_millis(250);
/// The QR's side.
const QR_SIDE: f32 = 176.;
/// The countdown ring's diameter.
const RING: f32 = 14.;
/// The longest device name shown; the daemon bounds it too.
const NAME_LIMIT: usize = 64;

/// The open pairing dialog. Dropping it, as closing the dialog does, cancels
/// a pairing still open at the daemon, so its token stops working at once.
pub(crate) struct PairingDialog {
    pub(crate) pairing: Pairing,
    client: Option<Client>,
    _lease: Option<Lease>,
    _task: Option<Task<()>>,
}

impl Drop for PairingDialog {
    fn drop(&mut self) {
        if let (Some(client), Some(id)) = (&self.client, self.pairing.id())
            && self.pairing.cancels_on_close()
        {
            client.notify(Request::CancelPairing { id: id.clone() });
        }
    }
}

impl HerdrWindow {
    /// Opens the pairing dialog, unless another page holds the window.
    pub(crate) fn open_remote_pairing(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() || !self.open_menu(window, cx) {
            return false;
        }
        let client = Supervisor::client(cx);
        let pairing = match (&client, self.config.remote.enabled) {
            (_, false) => Pairing::new(false),
            (Some(_), true) => Pairing::new(true),
            (None, true) => Pairing::failed("Remote control is not available in this window."),
        };
        let task = client
            .clone()
            .filter(|_| self.config.remote.enabled)
            .map(|client| Self::drive_pairing(client, cx));
        self.menu.remote_pairing = Some(PairingDialog {
            pairing,
            client,
            _lease: Supervisor::lease(cx),
            _task: task,
        });
        self.menu.page = Some(Page::RemoteControl);
        cx.notify();
        true
    }

    /// Sends what the pairing asks for, one request at a time, until the
    /// dialog closes.
    fn drive_pairing(client: Client, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                let next = this.update(cx, |view, cx| {
                    let dialog = view.menu.remote_pairing.as_mut()?;
                    let now = Instant::now();
                    let ticket = dialog.pairing.next(now);
                    if ticket.is_none() && dialog.pairing.countdown(now).is_some() {
                        cx.notify();
                    }
                    Some(ticket)
                });
                let ticket = match next {
                    Ok(Some(Some(ticket))) => ticket,
                    Ok(Some(None)) => {
                        cx.background_executor().timer(STEP).await;
                        continue;
                    }
                    Ok(None) | Err(_) => return,
                };
                let result = client.request(ticket.request.clone()).await;
                let open = this.update(cx, |view, cx| {
                    let Some(dialog) = view.menu.remote_pairing.as_mut() else {
                        return false;
                    };
                    dialog.pairing.receive(&ticket, result, Instant::now());
                    if matches!(dialog.pairing.phase(), Phase::Approved { .. }) {
                        Supervisor::refresh(cx);
                    }
                    cx.notify();
                    true
                });
                if !matches!(open, Ok(true)) {
                    return;
                }
            }
        })
    }

    fn answer_pairing(&mut self, allow: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.menu.remote_pairing {
            dialog.pairing.answer(allow);
            cx.notify();
        }
    }

    fn restart_pairing(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.menu.remote_pairing {
            dialog.pairing.restart();
            cx.notify();
        }
    }

    /// Whether a remote control dialog is open, for the menu's layout.
    pub(crate) fn remote_dialog_open(&self) -> bool {
        matches!(
            self.menu.page,
            Some(Page::RemoteControl | Page::RevokeDevice)
        )
    }

    /// The open remote control dialog's body.
    pub(crate) fn remote_page(&self, cx: &mut Context<Self>) -> Option<Div> {
        match self.menu.page {
            Some(Page::RemoteControl) => Some(self.render_pairing(cx)),
            Some(Page::RevokeDevice) => Some(self.render_revoke(cx)),
            _ => None,
        }
    }

    fn render_pairing(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(dialog) = &self.menu.remote_pairing else {
            return div();
        };
        let body = match dialog.pairing.phase() {
            Phase::Off => self.pairing_message(
                "Remote control is off.",
                "Turn it on in Settings > Remote control to pair a phone.",
                Some(("remote-pair-settings", "Open Settings", Action::Settings)),
                cx,
            ),
            Phase::Requesting => self.pairing_offer(None, cx),
            Phase::Showing { .. } => self.pairing_offer(Some(&dialog.pairing), cx),
            Phase::Claimed {
                device_name,
                code,
                answer,
                ..
            } => self.pairing_claim(device_name, code, *answer, cx),
            Phase::Approved { device_name } => self.pairing_message(
                &format!("{} can now control your chats.", name(device_name)),
                "Revoke it any time in Settings > Remote control.",
                Some(("remote-pair-done", "Done", Action::Close)),
                cx,
            ),
            Phase::Denied => self.pairing_message(
                "Not paired.",
                "The device was turned away and its code no longer works.",
                Some(("remote-pair-close", "Close", Action::Close)),
                cx,
            ),
            Phase::Expired { device_name } => self.pairing_message(
                &format!("The request from {} expired.", name(device_name)),
                "Nothing was granted. Scan a new code to try again.",
                Some(("remote-pair-again", "New code", Action::Restart)),
                cx,
            ),
            Phase::Failed { message } => self.pairing_message(
                "Pairing is unavailable.",
                &crate::notifications::safe_text(message, 300),
                Some(("remote-pair-retry", "Try again", Action::Restart)),
                cx,
            ),
        };
        div()
            .debug_selector(|| "remote-pair".into())
            .flex()
            .flex_col()
            .min_h_0()
            .p(px(8.))
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Remote control"),
                    )
                    .child(
                        div()
                            .id("remote-pair-x")
                            .debug_selector(|| "remote-pair-x".into())
                            .role(Role::Button)
                            .aria_label("Close")
                            .p(px(4.))
                            .rounded(px(crate::config::corners::SMALL))
                            .cursor_pointer()
                            .hover(|button| button.bg(rgb(theme.active)))
                            .child(
                                svg()
                                    .path("icons/x.svg")
                                    .size(px(12.))
                                    .text_color(rgb(theme.subtext())),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    ),
            )
            .child(body)
    }

    /// The QR with what the phone needs beside it; a placeholder square
    /// until the daemon has answered.
    fn pairing_offer(&self, pairing: Option<&Pairing>, _cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let showing = pairing.and_then(|pairing| match pairing.phase() {
            Phase::Showing { offer, qr, .. } => Some((offer, qr.clone())),
            _ => None,
        });
        let countdown = pairing.and_then(|pairing| pairing.countdown(Instant::now()));
        let square = match &showing {
            Some((_, qr)) => qr_square(qr.clone()).into_any_element(),
            None => div()
                .size(px(QR_SIDE))
                .rounded(px(crate::config::corners::CONTROL))
                .bg(rgb(theme.active))
                .into_any_element(),
        };
        let details = match &showing {
            Some((offer, _)) => div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Scan with your phone's camera"),
                )
                .child(
                    div()
                        .debug_selector(|| "remote-pair-addresses".into())
                        .text_color(rgb(theme.subtext()))
                        .child(addresses(&offer.addresses)),
                )
                .child(
                    div()
                        .debug_selector(|| "remote-pair-fingerprint".into())
                        .text_font(&self.config.terminal)
                        .text_size(px(10.))
                        .text_color(rgb(theme.subtext()))
                        .child(format!(
                            "CA {}",
                            crate::notifications::safe_text(&offer.fingerprint, 200)
                        )),
                )
                .children(countdown.map(|(left, share)| {
                    div()
                        .debug_selector(|| "remote-pair-countdown".into())
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_color(rgb(theme.subtext()))
                        .child(ring(share, crate::menu::accent(theme), rgb(theme.active)))
                        .child(format!(
                            "New code in {} s",
                            left.as_secs_f32().ceil() as u64
                        ))
                })),
            None => div()
                .text_color(rgb(theme.subtext()))
                .child("Asking for a pairing code\u{2026}"),
        };
        div()
            .flex()
            .gap(px(16.))
            .items_start()
            .child(div().flex_none().child(square))
            .child(div().flex_1().min_w_0().child(details))
    }

    fn pairing_claim(
        &self,
        device_name: &str,
        code: &str,
        answer: Option<Answer>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let waiting = answer.is_some();
        div()
            .debug_selector(|| "remote-pair-claim".into())
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .child(
                        div()
                            .debug_selector(|| "remote-pair-device".into())
                            .font_weight(FontWeight::BOLD)
                            .child(name(device_name)),
                    )
                    .child("wants remote control of your chats."),
            )
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child("Allow it only if the phone shows the same code."),
            )
            .child(
                div()
                    .debug_selector(|| "remote-pair-code".into())
                    .text_font(&self.config.terminal)
                    .text_size(px(28.))
                    .line_height(px(36.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(spaced(code)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button(theme, "remote-pair-deny", "Deny", Tone::Plain, !waiting).when(
                            !waiting,
                            |button| {
                                button.on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.answer_pairing(false, cx);
                                }))
                            },
                        ),
                    )
                    .child(
                        button(
                            theme,
                            "remote-pair-allow",
                            if waiting { "Allowing\u{2026}" } else { "Allow" },
                            Tone::Primary,
                            !waiting,
                        )
                        .when(!waiting, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.answer_pairing(true, cx);
                            }))
                        }),
                    ),
            )
    }

    fn pairing_message(
        &self,
        title: &str,
        detail: &str,
        action: Option<(&'static str, &'static str, Action)>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        div()
            .debug_selector(|| "remote-pair-message".into())
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(detail.to_owned()),
            )
            .children(action.map(|(id, label, action)| {
                div().flex().justify_end().child(
                    button(theme, id, label, Tone::Primary, true).on_click(cx.listener(
                        move |this, _, window, cx| {
                            cx.stop_propagation();
                            match action {
                                Action::Close => this.dismiss_menu(window, cx),
                                Action::Restart => this.restart_pairing(cx),
                                Action::Settings => {
                                    this.dismiss_menu(window, cx);
                                    crate::settings_window::open_remote(
                                        cx.entity().downgrade(),
                                        cx,
                                    );
                                }
                            }
                        },
                    )),
                )
            }))
    }
}

/// What a message's button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Close,
    Restart,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Plain,
    Primary,
    Danger,
}

/// A dialog button; the caller adds `.on_click` when it is enabled.
pub(super) fn button(
    theme: &Theme,
    id: &'static str,
    label: &'static str,
    tone: Tone,
    enabled: bool,
) -> Stateful<Div> {
    let button = div()
        .id(id)
        .debug_selector(move || id.into())
        .role(Role::Button)
        .aria_label(label);
    crate::accessibility::disabled(button, !enabled)
        .px(px(12.))
        .py(px(6.))
        .rounded(px(crate::config::corners::CONTROL))
        .when(tone != Tone::Plain, |button| button.bg(rgb(theme.active)))
        .when(tone == Tone::Danger, |button| {
            button.text_color(crate::menu::danger(theme))
        })
        .when(tone == Tone::Primary, |button| {
            button.text_color(crate::menu::accent(theme))
        })
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|button| button.bg(rgb(theme.active)))
        })
        .when(!enabled, |button| button.opacity(0.5))
        .child(label)
}

/// A device name from a phone, made safe to draw.
fn name(device_name: &str) -> String {
    let name = crate::notifications::safe_text(device_name.trim(), NAME_LIMIT);
    if name.is_empty() {
        "A device".to_owned()
    } else {
        name
    }
}

/// The origins a phone may reach this computer at, without their scheme.
pub(crate) fn addresses(origins: &[String]) -> String {
    origins
        .iter()
        .map(|origin| origin.strip_prefix("https://").unwrap_or(origin))
        .map(|origin| crate::notifications::safe_text(origin, 100))
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

/// The QR painted as quads: dark runs on a light square, whatever the
/// theme, since scanners need the contrast.
fn qr_square(qr: Arc<Qr>) -> impl IntoElement {
    let painted = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let side = f32::from(bounds.size.width.min(bounds.size.height));
            let Some(layout) = qr::layout(side, qr.width()) else {
                return;
            };
            let at = |x: f32, y: f32, width: f32, height: f32| {
                Bounds::new(
                    point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
                    size(px(width), px(height)),
                )
            };
            window.paint_quad(fill(
                at(layout.margin, layout.margin, layout.size, layout.size),
                rgb(0xffffff),
            ));
            for run in qr.runs() {
                window.paint_quad(fill(
                    at(
                        layout.origin + run.column as f32 * layout.module,
                        layout.origin + run.row as f32 * layout.module,
                        run.length as f32 * layout.module,
                        layout.module,
                    ),
                    rgb(0x000000),
                ));
            }
        },
    )
    .size_full();
    div()
        .debug_selector(|| "remote-pair-qr".into())
        .size(px(QR_SIDE))
        .child(painted)
}

/// The countdown: a faint full circle and, over it, the share left.
fn ring(share: f32, color: Rgba, track: Rgba) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let radius = f32::from(bounds.size.width.min(bounds.size.height)) / 2. - 1.5;
            let center = (
                f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.,
                f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.,
            );
            for (share, color) in [(1., track), (share, color)] {
                let points = arc(center, radius, share, 48);
                let Some((first, rest)) = points.split_first() else {
                    continue;
                };
                let mut path = PathBuilder::stroke(px(2.));
                path.move_to(point(px(first.0), px(first.1)));
                for (x, y) in rest {
                    path.line_to(point(px(*x), px(*y)));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .size(px(RING))
    .flex_none()
}

/// Points along a circle from twelve o'clock, clockwise, for `share` of a
/// turn: `segments` for a whole turn, at least two for any part. Empty for
/// nothing left.
pub(crate) fn arc(center: (f32, f32), radius: f32, share: f32, segments: usize) -> Vec<(f32, f32)> {
    let share = if share.is_finite() {
        share.clamp(0., 1.)
    } else {
        0.
    };
    if share <= 0. || segments == 0 || radius <= 0. {
        return Vec::new();
    }
    let steps = ((segments as f32 * share).ceil() as usize).max(1);
    (0..=steps)
        .map(|step| {
            let angle = std::f32::consts::TAU * share * step as f32 / steps as f32
                - std::f32::consts::FRAC_PI_2;
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
