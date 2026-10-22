//! Config diagnostics drawn like notification cards over the top-right of the
//! terminal area, where Herdr draws its own: this app's GUI config warning,
//! the missing icon font, then the selected endpoint's daemon `config.toml`
//! diagnostic.
use super::{Flash, HerdrWindow, chrome_focus::Region, render::Notice};
use crate::{fonts::StyledFont, notifications::safe_text};
use gpui::{prelude::*, *};
use std::sync::Arc;

mod fold;

pub(crate) use fold::Overflow;

const MAX_WIDTH: f32 = 420.;

/// One card's text and state, as [`HerdrWindow::render_diagnostic_card`]
/// draws it.
struct Card<'a> {
    selector: &'static str,
    label: String,
    lines: &'a Arc<[String]>,
    expanded: bool,
    /// Spans the card under its text, for a control the lines cannot carry.
    footer: Option<AnyElement>,
}

impl HerdrWindow {
    /// This app's GUI config warning and icon font notice, then the selected
    /// daemon's diagnostic.
    pub(super) fn config_diagnostic_cards(&self, cx: &mut Context<Self>) -> Vec<Notice> {
        let gui = self.gui_config_diagnostic.visible().map(|lines| {
            let (dismissed, toggled) = (lines.clone(), lines.clone());
            self.render_diagnostic_card(
                Card {
                    selector: "gui-config-diagnostic",
                    label: "Herdr UI".into(),
                    lines,
                    expanded: self.gui_config_diagnostic.expanded(),
                    footer: None,
                },
                move |this, cx| {
                    if this.gui_config_diagnostic.dismiss(&dismissed) {
                        cx.notify();
                    }
                },
                move |this, cx| {
                    if this.gui_config_diagnostic.toggle(&toggled) {
                        cx.notify();
                    }
                },
                cx,
            )
        });
        let icon_font = self.icon_font_notice.visible().map(|lines| {
            let (dismissed, toggled) = (lines.clone(), lines.clone());
            let command =
                crate::icon_font_notice::COMMAND.map(|command| self.command_chip(command, cx));
            self.render_diagnostic_card(
                Card {
                    selector: "icon-font-notice",
                    label: "Herdr UI".into(),
                    lines,
                    expanded: self.icon_font_notice.expanded(),
                    footer: command,
                },
                move |this, cx| {
                    if this.icon_font_notice.dismiss(&dismissed) {
                        cx.notify();
                    }
                },
                move |this, cx| {
                    if this.icon_font_notice.toggle(&toggled) {
                        cx.notify();
                    }
                },
                cx,
            )
        });
        let endpoint = self
            .endpoints
            .get(self.selected_endpoint)
            .and_then(|endpoint| {
                let lines = endpoint.config_diagnostic.visible()?;
                let source = DrawnFrom {
                    endpoint_id: endpoint.id.clone(),
                    generation: endpoint.generation,
                    inbox: endpoint.connection.inbox.clone(),
                    lines: lines.clone(),
                };
                let toggled = source.clone();
                Some(self.render_diagnostic_card(
                    Card {
                        selector: "config-diagnostic",
                        label: safe_text(&endpoint.label, 80),
                        lines,
                        expanded: endpoint.config_diagnostic.expanded(),
                        footer: None,
                    },
                    move |this, cx| {
                        this.dismiss_config_diagnostic(
                            &source.endpoint_id,
                            source.generation,
                            &source.inbox,
                            &source.lines,
                            cx,
                        );
                    },
                    move |this, cx| {
                        if toggled
                            .diagnostic(this)
                            .is_some_and(|d| d.toggle(&toggled.lines))
                        {
                            cx.notify();
                        }
                    },
                    cx,
                ))
            });
        gui.into_iter().chain(icon_font).chain(endpoint).collect()
    }

    /// A command the card names, in the terminal face on its own chip so it
    /// reads as something to run. Card text cannot be selected, so a click
    /// anywhere on the chip copies the command, as its icon says; it is never
    /// run. A narrow card truncates the text and still copies all of it.
    fn command_chip(&self, command: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let chip = div()
            .id("diagnostic-command")
            .debug_selector(|| "diagnostic-command".into())
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.))
            .p(px(8.))
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(self.theme.active))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                cx.write_to_clipboard(ClipboardItem::new_string(command.into()));
                this.show_flash(Flash::success("Copied the command"), cx);
            }))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_font(&self.config.terminal)
                    .child(command),
            )
            .child(
                div()
                    .debug_selector(|| "diagnostic-command-copy".into())
                    .flex_none()
                    .child(
                        svg()
                            .path("icons/copy.svg")
                            .size(px(12.))
                            .text_color(rgb(self.theme.foreground)),
                    ),
            );
        // A row, so the chip keeps to its text and shrinks with the card. It
        // starts where the text does, past the accent dot and its gap.
        div()
            .min_w_0()
            .pl(px(14.))
            .flex()
            .child(chip)
            .into_any_element()
    }

    /// A card is one row per line. When a row is cut short, the card offers
    /// to open: a click anywhere on it, or Enter or Space on its chevron,
    /// wraps every line in full, scrolling past [`fold::MAX_OPEN_HEIGHT`].
    /// A card whose rows all fit draws exactly as before, with no chevron.
    /// An open card gives up height in a short window, down to none for its
    /// lines, which then scroll.
    fn render_diagnostic_card(
        &self,
        card: Card<'_>,
        dismiss: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        toggle: impl Fn(&mut Self, &mut Context<Self>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> Notice {
        let Card {
            selector,
            label,
            lines,
            expanded,
            footer,
        } = card;
        let accent = self.theme.ink(self.theme.palette[3]);
        let foldable = self.notice_overflow.contains(selector);
        let open = foldable && expanded;
        // Card text is painted, which assistive technology never sees, so the
        // card is named with all of it, cut short on screen or not.
        let name = std::iter::once(label.as_str())
            .chain(lines.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(". ");
        let row = move |text: SharedString| {
            let row = div().min_w_0();
            if open {
                row.child(text)
            } else {
                row.truncate().child(text)
            }
        };
        let text = div()
            .flex_1()
            .min_w_0()
            .relative()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                row(label.clone().into())
                    .flex_none()
                    .text_color(rgb(self.theme.muted)),
            )
            .child(
                div()
                    .id("lines")
                    .debug_selector(move || format!("{selector}-lines"))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .when(open, |lines| {
                        lines
                            .min_h_0()
                            .max_h(px(fold::MAX_OPEN_HEIGHT))
                            .overflow_y_scroll()
                    })
                    .children(lines.iter().enumerate().map(|(index, line)| {
                        row(line.clone().into())
                            .debug_selector(move || format!("{selector}-line-{index}"))
                    })),
            )
            .child(fold::measure(fold::Measure {
                card: selector,
                label: label.into(),
                lines: lines.clone(),
                font: self.config.ui.font(),
                size: px(self.config.ui.size),
                foldable,
                overflow: self.notice_overflow.clone(),
            }));
        let view = cx.entity().downgrade();
        let chevron = foldable.then(|| {
            let (clicked, assisted) = (toggle.clone(), toggle.clone());
            let control = fold::chevron(selector, open, &self.theme).on_click(cx.listener(
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    clicked(this, cx);
                },
            ));
            let control =
                crate::accessibility::expandable(control, open, view, move |this, _, cx| {
                    assisted(this, cx)
                });
            self.chrome_stop(Region::Notices, control)
        });
        let dismiss = div()
            .id("dismiss")
            .debug_selector(move || format!("{selector}-dismiss"))
            .role(Role::Button)
            .aria_label("Dismiss")
            .flex_none()
            .size(px(fold::CONTROL))
            .rounded(px(crate::config::corners::CONTROL))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(self.theme.active)))
            .child(
                svg()
                    .path("icons/close.svg")
                    .size(px(12.))
                    .text_color(rgb(self.theme.foreground)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                dismiss(this, cx);
            }));
        let header = div()
            .min_w_0()
            .when(open, |header| header.min_h_0())
            .flex()
            .gap(px(8.))
            .child(
                div()
                    .mt(px(6.))
                    .size(px(6.))
                    .flex_none()
                    .rounded_full()
                    .bg(rgb(accent)),
            )
            .child(text)
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(px(fold::CONTROL_GAP))
                    .children(chevron)
                    .child(self.chrome_stop(Region::Notices, dismiss)),
            );
        let card = div()
            .id(selector)
            .debug_selector(move || selector.into())
            .role(Role::Group)
            .aria_label(name)
            .occlude()
            .min_w_0()
            .max_w(px(MAX_WIDTH))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(crate::config::corners::PANEL))
            .border_1()
            .border_color(rgb(accent))
            .bg(rgb(self.theme.surface))
            .text_color(rgb(self.theme.foreground))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            // The card itself has nothing else to do on a click, so all of it
            // opens and closes; its controls stop their own clicks first.
            .when(foldable, |card| {
                card.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| toggle(this, cx)))
            })
            .child(header)
            .children(footer.map(|footer| div().flex_none().child(footer)));
        Notice {
            card: card.into_any_element(),
            shrinks: open,
        }
    }

    /// Dismisses the banner only for the connection and text it was drawn
    /// from: a click delivered after either changed does nothing.
    fn dismiss_config_diagnostic(
        &mut self,
        endpoint_id: &str,
        generation: u64,
        inbox: &Arc<std::sync::Mutex<crate::state::LiveState>>,
        lines: &Arc<[String]>,
        cx: &mut Context<Self>,
    ) {
        let source = DrawnFrom {
            endpoint_id: endpoint_id.to_owned(),
            generation,
            inbox: inbox.clone(),
            lines: lines.clone(),
        };
        if source.diagnostic(self).is_some_and(|d| d.dismiss(lines)) {
            cx.notify();
        }
    }
}

/// The connection and text an endpoint's card was drawn from, so a click
/// delivered after either changed reaches nothing.
#[derive(Clone)]
struct DrawnFrom {
    endpoint_id: String,
    generation: u64,
    inbox: Arc<std::sync::Mutex<crate::state::LiveState>>,
    lines: Arc<[String]>,
}

impl DrawnFrom {
    fn diagnostic<'a>(
        &self,
        window: &'a mut HerdrWindow,
    ) -> Option<&'a mut crate::config_diagnostic::ConfigDiagnostic> {
        window
            .endpoints
            .iter_mut()
            .find(|e| {
                e.id == self.endpoint_id
                    && e.generation == self.generation
                    && Arc::ptr_eq(&e.connection.inbox, &self.inbox)
            })
            .map(|endpoint| &mut endpoint.config_diagnostic)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
