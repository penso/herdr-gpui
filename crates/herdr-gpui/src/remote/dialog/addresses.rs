//! The recommended connection stays visible while alternatives expand below it.
use super::*;
use crate::{config::corners, motion, notifications::safe_text};
use herdr_remote_control::control::{AddressKind, Offer};

const LIST_HEIGHT: f32 = 132.;

fn primary(offer: &Offer) -> usize {
    offer
        .addresses
        .iter()
        .position(|origin| kind(offer, origin) == AddressKind::Tailscale)
        .unwrap_or(0)
}

fn kind(offer: &Offer, origin: &str) -> AddressKind {
    offer
        .address_details
        .get(origin)
        .map(|details| details.kind)
        .unwrap_or_default()
}

fn interface(offer: &Offer, origin: &str) -> String {
    offer
        .address_details
        .get(origin)
        .filter(|details| !details.interfaces.is_empty())
        .map(|details| safe_text(&details.interfaces.join(", "), 160))
        .unwrap_or_else(|| "Interface unavailable".into())
}

fn hint(kind: AddressKind) -> &'static str {
    match kind {
        AddressKind::Tailscale => {
            "Connect both devices to the same Tailscale network. Works across networks when access is allowed."
        }
        AddressKind::Local => "Same local network; network isolation may block access.",
        AddressKind::Bridge => "Local bridge; phone reachability is not verified.",
        AddressKind::Loopback => "Only reachable on this computer, not from your phone.",
        AddressKind::Other => "Phone reachability depends on your network.",
    }
}

impl PairingDialog {
    pub(super) fn reveal_selected_address(&mut self) {
        let Phase::Showing { offer, .. } = self.pairing.phase() else {
            return;
        };
        let Some(index) = offer
            .addresses
            .iter()
            .position(|origin| offer.url.starts_with(&format!("{origin}/")))
        else {
            return;
        };
        let primary = primary(offer);
        if index != primary {
            self.other_addresses.set(true, motion::now(), true);
            self.addresses_scroll
                .scroll_to_item(index - usize::from(index > primary));
        } else {
            self.addresses_scroll.scroll_to_item(0);
        }
    }
}

impl HerdrWindow {
    pub(super) fn set_other_addresses(&mut self, open: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.menu.remote_pairing {
            dialog.other_addresses.set(open, motion::now(), cfg!(test));
            if !cfg!(test) {
                dialog._addresses_task = Some(cx.spawn(async move |this, cx| {
                    loop {
                        let moving = this.update(cx, |view, cx| {
                            cx.notify();
                            view.menu
                                .remote_pairing
                                .as_ref()
                                .is_some_and(|dialog| dialog.other_addresses.moving(motion::now()))
                        });
                        if !matches!(moving, Ok(true)) {
                            break;
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;
                    }
                }));
            }
            cx.notify();
        }
    }

    pub(super) fn pairing_selected_address(&self, offer: &Offer) -> Div {
        let selected = offer
            .addresses
            .iter()
            .find(|origin| offer.url.starts_with(&format!("{origin}/")));
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .when_some(selected, |summary, origin| {
                summary
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("QR uses {}", kind(offer, origin).label())),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(self.theme.subtext()))
                            .child(interface(offer, origin)),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .child(addresses(std::slice::from_ref(origin))),
                    )
            })
    }

    pub(super) fn pairing_addresses(&self, offer: &Offer, cx: &mut Context<Self>) -> Div {
        let Some(dialog) = &self.menu.remote_pairing else {
            return div();
        };
        if offer.addresses.is_empty() {
            return div();
        }
        let primary = primary(offer);
        let origin = &offer.addresses[primary];
        let theme = &self.theme;
        let tailscale = kind(offer, origin) == AddressKind::Tailscale;
        let now = motion::now();
        let shown = dialog.other_addresses.shown(now);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .when(offer.address_details.is_empty(), |body| {
                body.child(div()
                    .debug_selector(|| "remote-pair-older-daemon".into())
                    .text_size(px(11.)).text_color(rgb(theme.subtext()))
                    .child("Network labels need the updated remote-control daemon. Restart it in Settings > Remote control, then reopen pairing."))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(corners::CONTROL))
                    .bg(rgb(theme.surface))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .items_center()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(kind(offer, origin).label()),
                            )
                            .when(tailscale, |row| {
                                row.child(
                                    div()
                                        .text_size(px(10.))
                                        .px(px(5.))
                                        .py(px(2.))
                                        .rounded(px(corners::SMALL))
                                        .bg(rgb(theme.primary_wash()))
                                        .text_color(rgb(theme.ink(theme.primary())))
                                        .child("Recommended"),
                                )
                            }),
                    )
                    .child(self.pairing_address_row(offer, primary, false, cx))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme.subtext()))
                            .child(hint(kind(offer, origin))),
                    ),
            )
            .when(offer.addresses.len() > 1, |body| {
                body.child(
                    div()
                        .id("remote-pair-other-addresses")
                        .debug_selector(|| "remote-pair-other-addresses".into())
                        .role(Role::Button)
                        .aria_label(if dialog.other_addresses.is_open() {
                            "Hide other addresses"
                        } else {
                            "Show other addresses"
                        })
                        .flex()
                        .gap(px(6.))
                        .items_center()
                        .py(px(4.))
                        .cursor_pointer()
                        .text_color(rgb(theme.ink(theme.primary())))
                        .child(
                            svg()
                                .path(if dialog.other_addresses.is_open() {
                                    "icons/chevron-up.svg"
                                } else {
                                    "icons/chevron-down.svg"
                                })
                                .size(px(12.))
                                .text_color(rgb(theme.ink(theme.primary()))),
                        )
                        .child(format!("Other addresses ({})", offer.addresses.len() - 1))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            let open = this
                                .menu
                                .remote_pairing
                                .as_ref()
                                .is_some_and(|dialog| dialog.other_addresses.is_open());
                            this.set_other_addresses(!open, cx);
                        })),
                )
                .when(shown > 0., |body| {
                    body.child(
                        div().h(px(LIST_HEIGHT * shown)).overflow_hidden().child(
                            div()
                                .id("remote-pair-addresses")
                                .debug_selector(|| "remote-pair-addresses".into())
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .h(px(LIST_HEIGHT))
                                .overflow_y_scroll()
                                .track_scroll(&dialog.addresses_scroll)
                                .children(
                                    (0..offer.addresses.len())
                                        .filter(|index| *index != primary)
                                        .map(|index| {
                                            self.pairing_address_row(offer, index, true, cx)
                                        }),
                                ),
                        ),
                    )
                })
            })
    }

    fn pairing_address_row(
        &self,
        offer: &Offer,
        index: usize,
        detailed: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let origin = &offer.addresses[index];
        let selected = offer.url.starts_with(&format!("{origin}/"));
        let label = addresses(std::slice::from_ref(origin));
        let network = kind(offer, origin);
        let interface = interface(offer, origin);
        let address = origin.clone();
        div()
            .id(("remote-pair-address", index))
            .debug_selector(move || format!("remote-pair-address-{index}"))
            .role(Role::Button)
            .aria_label(format!(
                "{} {} · {} · {}",
                if selected { "Selected:" } else { "Use" },
                network.label(),
                interface,
                label
            ))
            .flex_none()
            .flex()
            .gap(px(8.))
            .items_center()
            .p(px(8.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(if selected {
                theme.primary()
            } else {
                theme.active
            }))
            .bg(rgb(if selected {
                theme.primary_wash()
            } else {
                theme.background
            }))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.active)))
            .child(
                svg()
                    .path("icons/check.svg")
                    .size(px(12.))
                    .flex_none()
                    .text_color(rgb(theme.ink(theme.primary())))
                    .when(!selected, |icon| icon.invisible()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .min_w_0()
                    .flex_1()
                    .when(detailed, |body| {
                        body.child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(network.label()),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme.subtext()))
                            .child(interface),
                    )
                    .child(
                        div()
                            .text_font(&self.config.terminal)
                            .text_size(px(12.))
                            .child(label),
                    )
                    .when(detailed, |body| {
                        body.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(theme.subtext()))
                                .child(hint(network)),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select_pairing_address(&address, cx);
            }))
    }
}
