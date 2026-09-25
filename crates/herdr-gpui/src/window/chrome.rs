//! The kit chrome around the terminal: the tab strip above it, the status bar
//! below it, and the flash over its cells. Everything here renders from
//! prepared state and reads colors from the kit theme.

use super::{HerdrWindow, flash::Tone};
use crate::{
    APP_VERSION, controls::Command, navigation::NavigationTarget, state::ConnectionStatus,
};
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    spinner::Spinner,
    status_bar::StatusBar,
    tab::{Tab, TabBar},
    tag::Tag,
};
use gpui_kit::{
    Context, Div, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    SharedString, Styled, div, prelude::FluentBuilder, px,
};

/// Long tab labels truncate here instead of pushing their neighbours away.
const TAB_MAX_WIDTH: f32 = 220.;

impl HerdrWindow {
    /// The focused workspace's tabs, each closable, with a new-tab button.
    pub(super) fn render_tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut selected = None;
        let mut tabs = Vec::new();
        if let Some(snapshot) = &self.live.snapshot {
            for tab in snapshot
                .tabs
                .iter()
                .filter(|t| Some(&t.workspace_id) == snapshot.focused_workspace_id.as_ref())
            {
                if tab.focused {
                    selected = Some(tabs.len());
                }
                let id = tab.tab_id.clone();
                let (context_id, close_id) = (id.clone(), id.clone());
                tabs.push(
                    Tab::new()
                        .label(tab.label.clone())
                        .debug_selector({
                            let id = id.clone();
                            move || format!("tab-{id}")
                        })
                        .suffix(
                            Button::new(SharedString::from(format!("close-tab-{id}")))
                                .debug_selector({
                                    let id = id.clone();
                                    move || format!("close-tab-{id}")
                                })
                                .ghost()
                                .xsmall()
                                .icon(IconName::Close)
                                .tooltip("Close tab")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.open_tab_close(&close_id, window, cx);
                                })),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.open_tab_menu(&context_id, event.position, window, cx);
                            }),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate(NavigationTarget::Tab(&id), cx);
                            window.focus(&this.focus, cx);
                        })),
                );
            }
        }
        TabBar::new("tabs")
            .small()
            .max_width(px(TAB_MAX_WIDTH))
            .when_some(selected, |bar, index| bar.selected_index(index))
            .children(tabs)
            .suffix(
                Button::new("new-tab")
                    .debug_selector(|| "new-tab".into())
                    .ghost()
                    .small()
                    .icon(IconName::Plus)
                    .tooltip("New tab")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.command(Command::Tab, window, cx)),
                    ),
            )
    }

    /// Connection state on the left, derived from `ConnectionStatus` alone,
    /// then the IME preview, then the chrome's own shortcuts.
    pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let status = (!matches!(self.live.status, ConnectionStatus::Connected)
            || self.local_error.is_some()
            || self.live.error.is_some())
        .then(|| self.live.status_text(self.local_error.as_deref()));
        let indicator = (!self.live.status.is_connected()).then(|| {
            if matches!(self.live.status, ConnectionStatus::StartingDaemon) {
                Spinner::new()
                    .xsmall()
                    .color(cx.theme().warning)
                    .into_any_element()
            } else {
                Icon::new(IconName::CircleX)
                    .xsmall()
                    .text_color(cx.theme().danger)
                    .into_any_element()
            }
        });
        let update_available = self.updater.update_available();
        div()
            .id("connection-status")
            .debug_selector(|| "connection-status".into())
            .flex_none()
            .child(
                StatusBar::new()
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap_1()
                            .children(indicator)
                            .when_some(status, |row, status| {
                                row.child(
                                    div()
                                        .debug_selector(|| "connection-message".into())
                                        .min_w_0()
                                        .truncate()
                                        .child(status),
                                )
                            }),
                    )
                    .when(!self.marked.is_empty(), |bar| {
                        bar.child(
                            div()
                                .min_w_0()
                                .max_w(px(160.))
                                .truncate()
                                .child(format!("Composing: {}", self.marked)),
                        )
                    })
                    .right(
                        Button::new("status-theme")
                            .debug_selector(|| "status-theme".into())
                            .ghost()
                            .xsmall()
                            .icon(IconName::Palette)
                            .label("Theme")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_theme_picker(window, cx);
                            })),
                    )
                    .right(
                        Button::new("status-keybinds")
                            .debug_selector(|| "status-keybinds".into())
                            .ghost()
                            .xsmall()
                            .icon(Icon::empty().path("icons/keyboard.svg"))
                            .label("Shortcuts")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_keybinds(window, cx);
                            })),
                    )
                    .right(
                        Button::new("report-issue")
                            .debug_selector(|| "report-issue".into())
                            .ghost()
                            .xsmall()
                            .icon(IconName::ExternalLink)
                            .label("Report issue")
                            .on_click(|_, _, cx| {
                                cx.open_url(&format!(
                                    "https://github.com/penso/herdr-gpui/issues/new?template=bug_report.yml&version={}",
                                    APP_VERSION.replace('+', "%2B"),
                                ));
                            }),
                    )
                    .right(
                        // A waiting update is the one status here worth
                        // interrupting for, so it takes the primary variant.
                        Button::new("status-version")
                            .debug_selector(|| "status-version".into())
                            .xsmall()
                            .map(|button| {
                                if update_available {
                                    button.primary().label("Update available")
                                } else {
                                    button.ghost().label(APP_VERSION)
                                }
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_app_update(false, window, cx);
                            })),
                    ),
            )
    }

    /// Direct feedback for the user's own gesture, not a daemon notice: it
    /// sits over the cells it answers and needs no dismissing, so it stays a
    /// tag over the terminal rather than joining the notification stack.
    pub(super) fn render_flash(&self, sidebar_gap: f32) -> Option<Div> {
        use crate::config::ClipboardToastPosition::*;
        let (flash, _) = self.flash.as_ref()?;
        let position = self.config.clipboard_toast.position;
        let tag = match flash.tone {
            Tone::Success => Tag::success(),
            Tone::Warning => Tag::warning(),
        };
        Some(
            div()
                .absolute()
                .map(|row| match position {
                    TopLeft | TopCenter | TopRight => row.top(px(12.)),
                    BottomLeft | BottomCenter | BottomRight => row.bottom(px(12.)),
                })
                .map(|row| match position {
                    TopLeft | BottomLeft => row.justify_start(),
                    TopCenter | BottomCenter => row.justify_center(),
                    TopRight | BottomRight => row.justify_end(),
                })
                // The pane's own padding is not part of the terminal: the
                // flash spans the cells, so centering centers on them and a
                // corner is the corner of the grid.
                .left(px(sidebar_gap))
                .right_0()
                .px(px(12.))
                .flex()
                .overflow_hidden()
                .child(
                    div()
                        .debug_selector(|| "flash".into())
                        .min_w_0()
                        .child(tag.child(div().truncate().child(flash.text.clone()))),
                ),
        )
    }
}
