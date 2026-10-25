//! The status bar along the window's foot. `[status_bar]` chooses which of
//! its items show and how compactly, so a crowded bar can give room back.

use super::HerdrWindow;
use crate::{
    APP_VERSION,
    config::status_bar::{AwakeScope, Button},
    state::ConnectionStatus,
};
use gpui::{prelude::*, *};
use std::time::Duration;

/// The status bar's 24-unit SVG icons pad their artwork, so they are drawn at
/// this size to look as large as the 12px ring of the report-issue button.
const STATUS_GLYPH: f32 = 16.;

/// The widest the focused agent's model gets, so a long name cannot crowd out
/// the usage after it.
const MODEL_WIDTH: f32 = 160.;

impl HerdrWindow {
    /// The bar's height, which grows with the UI font.
    pub(super) fn status_bar_height(&self) -> f32 {
        (self.config.ui.size * 1.5 + 4.).max(22.)
    }

    pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let status = (!matches!(self.live.status, ConnectionStatus::Connected)
            || self.local_error.is_some()
            || self.live.error.is_some())
        .then(|| self.live.status_text(self.local_error.as_deref()));
        let items = self.config.status_bar;
        let height = px(self.status_bar_height());
        let usage = self.render_usage(cx);
        let has_usage = usage.is_some();
        div()
            .id("connection-status")
            .debug_selector(|| "connection-status".into())
            .role(Role::Status)
            .aria_label("Status bar")
            // Windows pages draw above the bar's tooltips, so those under
            // where a tooltip shows hide while the pointer is over the bar.
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.hover_status_bar(*hovered, cx);
            }))
            .relative()
            .children(self.status_bar_probe())
            .flex()
            .flex_none()
            .h(height)
            .overflow_hidden()
            .items_center()
            .gap(px(6.))
            .px_3()
            .bg(self.fill().panel(self.theme.surface))
            .text_color(rgb(self.theme.foreground))
            .children(self.render_agent_model(cx))
            .children(usage)
            .when_some(
                self.prefix_armed
                    .then(|| self.keymap().prefix_label())
                    .flatten(),
                |bar, prefix| {
                    bar.child(
                        div()
                            .debug_selector(|| "prefix-armed".into())
                            .flex_none()
                            .px(px(6.))
                            .rounded(px(crate::config::corners::SMALL))
                            .bg(rgb(self.theme.active))
                            .child(prefix),
                    )
                },
            )
            // The mode has no control on screen, so it says how it works.
            .when(self.resize_mode, |bar| {
                bar.child(
                    div()
                        .debug_selector(|| "resize-mode".into())
                        .flex_none()
                        .px(px(6.))
                        .rounded(px(crate::config::corners::SMALL))
                        .bg(rgb(self.theme.active))
                        .child("Resize"),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(self.theme.muted))
                        .child("h j k l or arrows resize, Esc ends"),
                )
            })
            .when(!self.live.status.is_connected(), |bar| {
                bar.child(self.connection_dot())
            })
            .child(
                div()
                    .when(!has_usage || status.is_some(), |row| row.flex_1())
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .when_some(status, |row, status| {
                        row.child(
                            div()
                                .id("connection-message")
                                .debug_selector(|| "connection-message".into())
                                .role(Role::Label)
                                .aria_value(status.clone())
                                .child(status),
                        )
                    }),
            )
            .children(self.render_listening_ports(cx))
            .children(self.render_system_load())
            .children(self.render_gpu_load())
            .children(self.render_network_quality(cx))
            .children(self.caffeine_button(items.keep_awake, cx))
            .when(items.theme.shown(), |bar| {
                bar.child(
                    self.status_button(
                        "status-theme",
                        glyph("icons/theme.svg", self.theme.foreground),
                        "Theme",
                        items.theme,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_theme_picker(window, cx);
                    })),
                )
            })
            .when(items.shortcuts.shown(), |bar| {
                bar.child(
                    self.status_button(
                        "status-keybinds",
                        glyph("icons/keyboard.svg", self.theme.foreground),
                        "Shortcuts",
                        items.shortcuts,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_keybinds(window, cx);
                    })),
                )
            })
            .when(items.report_issue.shown(), |bar| {
                bar.child(
                    self.status_button(
                        "report-issue",
                        self.issue_ring(),
                        "Report issue",
                        items.report_issue,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.open_report_issue(cx))),
                )
            })
            .child(
                div()
                    .id("status-version")
                    .debug_selector(|| "status-version".into())
                    .map(|control| {
                        self.chrome_stop(crate::window::chrome_focus::Region::StatusBar, control)
                    })
                    .role(Role::Button)
                    .aria_label(if self.updater.update_available() {
                        SharedString::from("Update available")
                    } else {
                        SharedString::from(format!("Version {APP_VERSION}"))
                    })
                    .flex_none()
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    // A waiting update is the one status here worth
                    // interrupting for, so it takes the accent color
                    // the rest of the chrome reserves for chosen rows.
                    .text_color(rgb(if self.updater.update_available() {
                        self.theme.primary()
                    } else {
                        self.theme.muted
                    }))
                    .child(if self.updater.update_available() {
                        "Update available"
                    } else {
                        APP_VERSION
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_app_update(false, window, cx);
                    })),
            )
    }

    /// The focused agent's model, from the `model` and `effort` tokens its
    /// integration reports. For Claude, clicking it opens Claude's own model
    /// picker; other agents show the model only.
    fn render_agent_model(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let agent = self.focused_agent()?;
        let label = model_label(agent)?;
        let picker = has_model_picker(agent);
        let segment = div()
            .id("agent-model")
            .debug_selector(|| "agent-model".into())
            .map(|segment| {
                // A label's name is its value, a button's its label.
                let spoken = format!("Model {label}");
                if picker {
                    self.chrome_stop(
                        crate::window::chrome_focus::Region::StatusBar,
                        segment.role(Role::Button).aria_label(spoken),
                    )
                } else {
                    segment.role(Role::Label).aria_value(spoken)
                }
            })
            .flex_none()
            .max_w(px(MODEL_WIDTH))
            .truncate()
            .px(px(6.))
            .rounded(px(crate::config::corners::CONTROL))
            .text_color(rgb(self.theme.muted))
            .child(label);
        if !picker {
            return Some(segment);
        }
        Some(
            segment
                .cursor_pointer()
                .hover(|s| s.bg(rgb(self.theme.active)))
                .tooltip(self.hint("Change model"))
                .on_click(cx.listener(|this, _, _, cx| this.open_agent_model_picker(cx))),
        )
    }

    fn focused_agent(&self) -> Option<&herdr_client::protocol::ClientShellAgent> {
        let snapshot = self.live.snapshot.as_ref()?;
        snapshot.agents.iter().find(|agent| agent.focused)
    }

    /// Types `/model` into the focused agent and focuses its pane, so the
    /// agent's own picker, with its full list and effort control, takes over.
    fn open_agent_model_picker(&mut self, cx: &mut Context<Self>) {
        let Some(pane) = self
            .focused_agent()
            .filter(|agent| has_model_picker(agent))
            .map(|agent| agent.pane_id.clone())
        else {
            return;
        };
        let Some(boot) = self.live.snapshot.as_ref().map(|s| s.boot_id.clone()) else {
            return;
        };
        let endpoint = self.endpoints[self.selected_endpoint].id.clone();
        let typed = self.endpoints[self.selected_endpoint]
            .connection
            .handle
            .as_ref()
            .ok_or(crate::Error::NotConnected)
            .and_then(|handle| Ok(handle.send_input(&boot, &pane, model_picker_input())?));
        if let Err(error) = typed {
            tracing::warn!(%error, "Could not open the agent's model picker");
            return;
        }
        self.navigate_endpoint(&endpoint, crate::NavigationTarget::Pane(&pane), cx);
    }

    fn connection_dot(&self) -> AnyElement {
        if matches!(self.live.status, ConnectionStatus::StartingDaemon) {
            div()
                .size(px(8.))
                .flex_none()
                .rounded_full()
                .bg(rgb(self.theme.ink(self.theme.palette[3])))
                .with_animation(
                    "daemon-starting-loader",
                    Animation::new(crate::motion::duration(Duration::from_secs(1))).repeat(),
                    |dot, delta| dot.opacity(0.3 + 0.7 * (delta * std::f32::consts::PI).sin()),
                )
                .into_any_element()
        } else {
            div()
                .size(px(6.))
                .flex_none()
                .rounded_full()
                .bg(rgb(self.theme.ink(self.theme.palette[1])))
                .into_any_element()
        }
    }

    /// Lets an `UntilAgentsFinish` hold go once no window's agents work.
    pub(crate) fn observe_agent_work(&self, window: &Window, cx: &mut Context<Self>) {
        let work = crate::caffeine::Work::of(
            self.endpoints
                .iter()
                .filter(|endpoint| endpoint.enabled)
                .map(|endpoint| {
                    endpoint
                        .live
                        .snapshot
                        .as_deref()
                        .filter(|_| endpoint.live.status.is_connected())
                }),
        );
        let report = Self::caffeine_report(cx);
        crate::caffeine::observe(window.window_handle().window_id(), work, cx, report);
    }

    /// Shows a keep-awake failure in this window, if it is still open.
    fn caffeine_report(cx: &Context<Self>) -> impl FnOnce(crate::Error, &mut App) + 'static {
        let view = cx.entity().downgrade();
        move |error, cx| {
            let flash = super::Flash::warning(error.to_string());
            let _ = view.update(cx, |this, cx| this.show_flash(flash, cx));
        }
    }

    /// Hidden by `keep_awake = false` only while off: the cup is the one
    /// way to let the display sleep again. A click holds until clicked again;
    /// a right-click holds until every agent finishes.
    fn caffeine_button(&self, shown: bool, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        use crate::caffeine::{Cup, Mode};
        let cup = crate::caffeine::cup(cx);
        if !shown && cup == Cup::Off {
            return None;
        }
        let (icon, color) = match cup {
            Cup::On(_) => ("icons/coffee-full.svg", self.theme.primary()),
            Cup::Pending => ("icons/coffee.svg", self.theme.muted),
            Cup::Off => ("icons/coffee.svg", self.theme.foreground),
        };
        let button = div()
            .id("status-caffeine")
            .debug_selector(|| "status-caffeine".into())
            .role(Role::Switch)
            .map(|control| {
                self.chrome_stop(crate::window::chrome_focus::Region::StatusBar, control)
            })
            .aria_label(match crate::caffeine::scope(cx) {
                AwakeScope::Display => "Keep the display awake",
                AwakeScope::System => "Keep the system awake",
            })
            .aria_toggled(match cup {
                Cup::On(_) => Toggled::True,
                Cup::Pending => Toggled::Mixed,
                Cup::Off => Toggled::False,
            })
            .when(cup == Cup::On(Mode::UntilAgentsFinish), |button| {
                button.aria_description("until agents finish")
            })
            .flex_none()
            .flex()
            .items_center()
            .px_2()
            .h_full()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(self.theme.active)))
            .child(div().relative().flex().child(glyph(icon, color)).when(
                cup == Cup::On(Mode::UntilAgentsFinish),
                // The dot marks a hold that ends on its own.
                |cup| {
                    cup.child(
                        div()
                            .id("status-caffeine-until-agents")
                            .debug_selector(|| "status-caffeine-until-agents".into())
                            .absolute()
                            .right(px(-1.))
                            .bottom(px(0.))
                            .size(px(6.))
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(self.theme.surface))
                            .bg(rgb(color)),
                    )
                },
            ))
            .tooltip(self.hint(match (cup, crate::caffeine::scope(cx)) {
                (Cup::On(Mode::UntilAgentsFinish), AwakeScope::Display) => {
                    "Keeping the display awake until agents finish"
                }
                (Cup::On(Mode::UntilAgentsFinish), AwakeScope::System) => {
                    "Keeping the system awake until agents finish"
                }
                (Cup::On(Mode::Indefinite), AwakeScope::Display) => {
                    "Keeping the display awake · Right-click: until agents finish"
                }
                (Cup::On(Mode::Indefinite), AwakeScope::System) => {
                    "Keeping the system awake · Right-click: until agents finish"
                }
                (Cup::Off | Cup::Pending, AwakeScope::Display) => {
                    "Keep the display awake · Right-click: until agents finish"
                }
                (Cup::Off | Cup::Pending, AwakeScope::System) => {
                    "Keep the system awake · Right-click: until agents finish"
                }
            }))
            .on_click(cx.listener(|_, _, _, cx| {
                let report = Self::caffeine_report(cx);
                crate::caffeine::toggle(cx, report);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|_, _, _, cx| {
                    cx.stop_propagation();
                    let report = Self::caffeine_report(cx);
                    crate::caffeine::toggle_until_agents_finish(cx, report);
                }),
            );
        Some(button)
    }

    /// The report-issue button's ring, drawn rather than an SVG.
    fn issue_ring(&self) -> AnyElement {
        div()
            .size(px(12.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(rgb(self.theme.foreground))
            .child(
                div()
                    .size(px(3.))
                    .rounded_full()
                    .bg(rgb(self.theme.foreground)),
            )
            .into_any_element()
    }

    /// A button with its name beside the icon, or the icon alone with the
    /// name in a tooltip. Labelled buttons yield their text to a crowded bar.
    fn status_button(
        &self,
        id: &'static str,
        icon: impl IntoElement,
        label: &'static str,
        style: Button,
    ) -> Stateful<Div> {
        let button = div()
            .id(id)
            .debug_selector(move || id.into())
            .role(Role::Button)
            .aria_label(label)
            .map(|control| {
                self.chrome_stop(crate::window::chrome_focus::Region::StatusBar, control)
            })
            .flex()
            .items_center()
            .gap(px(5.))
            .px_2()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(self.theme.active)))
            .child(icon);
        match style {
            Button::Label => button
                .flex_shrink_1()
                .min_w(px(33.))
                .child(div().min_w_0().truncate().child(label)),
            Button::Icon | Button::Hidden => button.flex_none().h_full().tooltip(self.hint(label)),
        }
    }

    fn hint(&self, text: &'static str) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let (foreground, surface) = (self.theme.foreground, self.theme.surface);
        move |_, cx| {
            cx.new(|_| crate::usage::Hint {
                text: text.into(),
                foreground,
                surface,
            })
            .into()
        }
    }
}

/// The agent's reported model, followed by its effort level when one is
/// reported too, or `None` when no model is reported.
pub(super) fn model_label(agent: &herdr_client::protocol::ClientShellAgent) -> Option<String> {
    let token = |key: &str| {
        agent
            .tokens
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    let model = token("model")?;
    Some(match token("effort") {
        Some(effort) => format!("{model} \u{b7} {effort}"),
        None => model.to_owned(),
    })
}

/// Whether typing `/model` in the agent opens its model picker. Only Claude's
/// is known; in another agent the text could be sent as a prompt.
pub(super) fn has_model_picker(agent: &herdr_client::protocol::ClientShellAgent) -> bool {
    agent.agent.as_deref() == Some("claude")
}

/// `/model` and Enter, which open Claude's full picker. The text goes after
/// an unsent prompt, if there is one. Alt+P keeps that prompt, but its picker
/// hides the effort control in a pane of fewer than about 40 rows.
fn model_picker_input() -> [herdr_client::protocol::ClientPaneInputEvent; 2] {
    [
        herdr_client::protocol::ClientPaneInputEvent::TextCommit("/model".into()),
        crate::menu::enter_key(),
    ]
}

fn glyph(path: &'static str, color: u32) -> Svg {
    svg()
        .path(path)
        .size(px(STATUS_GLYPH))
        .flex_none()
        .text_color(rgb(color))
}
