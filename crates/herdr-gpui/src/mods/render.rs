//! Draws mod trees with the app's theme and spacing. Rendering reads the
//! last tree a worker left, never Lua; a click becomes a catalog command or
//! a message queued for the mod's `update`.

use crate::{HerdrWindow, config::Theme, menu::accent, window::chrome_focus::Region};
use gpui::{prelude::*, *};
use herdr_mods::{
    Action, Align, Container, Data, Gap, IconSource, Node, Place, Size, Tone, Weight,
};

/// The widest one mod's status bar item gets.
const STATUS_ITEM_WIDTH: f32 = 280.;
/// The widest every mod's status bar items get together, so mods cannot
/// crowd out the app's own controls; the area shrinks before them.
const STATUS_AREA_WIDTH: f32 = 360.;
/// The share of the sidebar's height every mod section gets together, so
/// the workspace and agent lists always keep the rest.
const SIDEBAR_SHARE: f32 = 0.4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Axis {
    Row,
    Column,
}

impl HerdrWindow {
    /// The status bar items of the mods placed there, in name order.
    pub(crate) fn render_mod_status_items(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mut items = Vec::new();
        for (name, entry) in self.mods.entries() {
            if !entry.places().contains(&Place::StatusBar) {
                continue;
            }
            let Some(tree) = &entry.tree else {
                continue;
            };
            let painter = Painter {
                theme: &self.theme,
                name,
                size: self.config.ui.size,
                ring: self.mod_ring(Region::StatusBar),
            };
            let element = painter.node(tree, &format!("mod-{name}"), Axis::Row, cx);
            items.push(
                div()
                    .id(SharedString::from(format!("mod-status-{name}")))
                    .debug_selector(|| format!("mod-status-{name}"))
                    .flex()
                    .flex_shrink(1.)
                    .min_w_0()
                    .items_center()
                    .h_full()
                    .max_w(px(STATUS_ITEM_WIDTH))
                    .overflow_hidden()
                    .px(px(6.))
                    .child(element)
                    .into_any_element(),
            );
        }
        (!items.is_empty()).then(|| {
            div()
                .id("mod-status")
                .debug_selector(|| "mod-status".into())
                .flex()
                .flex_shrink(1.)
                .min_w_0()
                .max_w(px(STATUS_AREA_WIDTH))
                .h_full()
                .overflow_hidden()
                .children(items)
                .into_any_element()
        })
    }

    /// The mods placed in the sidebar, under the lists: a section each, its
    /// name then its tree, in one area that scrolls past
    /// [`SIDEBAR_SHARE`] of the sidebar's height.
    pub(crate) fn render_mod_sidebar_sections(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mut sections = Vec::new();
        for (name, entry) in self.mods.entries() {
            if !entry.places().contains(&Place::Sidebar) {
                continue;
            }
            let Some(tree) = &entry.tree else {
                continue;
            };
            let painter = Painter {
                theme: &self.theme,
                name,
                size: self.config.ui.size,
                ring: self.mod_ring(Region::Sidebar),
            };
            let body = painter.node(tree, &format!("mod-side-{name}"), Axis::Column, cx);
            sections.push(
                div()
                    .id(SharedString::from(format!("mod-sidebar-{name}")))
                    .debug_selector(|| format!("mod-sidebar-{name}"))
                    .flex()
                    .flex_col()
                    .flex_none()
                    .gap(px(4.))
                    .px(px(10.))
                    .py(px(6.))
                    .border_t_1()
                    .border_color(rgb(self.theme.active))
                    .child(
                        div()
                            .text_size(px(self.config.ui.size * 0.85))
                            .text_color(rgb(self.theme.muted))
                            .child(SharedString::from(name.to_owned())),
                    )
                    .child(div().text_color(rgb(self.theme.foreground)).child(body))
                    .into_any_element(),
            );
        }
        (!sections.is_empty()).then(|| {
            div()
                .id("mod-sidebar")
                .debug_selector(|| "mod-sidebar".into())
                .flex()
                .flex_col()
                .flex_shrink(1.)
                .min_h_0()
                .max_h(relative(SIDEBAR_SHARE))
                .overflow_y_scroll()
                .children(sections)
                .into_any_element()
        })
    }

    /// The focus ring for mod controls in `region`, while it holds the
    /// keyboard.
    pub(super) fn mod_ring(&self, region: Region) -> Option<Rgba> {
        (self.chrome_region == Some(region)).then(|| rgb(self.theme.primary()))
    }

    /// Runs what a mod's control asks for: a catalog command, as the
    /// palette runs it, or a message to the mod's `update`.
    pub(crate) fn mod_action(
        &mut self,
        name: &str,
        action: &Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            Action::Command(command) => self.run_mod_command(*command, window, cx),
            Action::Message(msg) => self.send_to_mod(name, msg.clone(), cx),
        }
    }

    /// Runs a catalog command a mod's control or `update` asked for. The
    /// command acts on the window's terminals and tabs, so a mod app shown
    /// over them steps aside first, or what the command opened stays hidden.
    pub(crate) fn run_mod_command(
        &mut self,
        command: crate::controls::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.leave_mod_app(window, cx);
        self.command(command, window, cx);
    }

    pub(crate) fn send_to_mod(&mut self, name: &str, msg: Data, cx: &mut Context<Self>) {
        if let Err(error) = self.mods.send(name, msg) {
            self.show_flash(crate::window::Flash::warning(error.to_string()), cx);
        }
    }

    fn open_mod_link(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(url) = crate::browser::WebUrl::try_from(url) else {
            return;
        };
        if crate::browser::EMBEDDED {
            // The tab opens in the window, which a mod app would cover.
            self.leave_mod_app(window, cx);
            self.open_browser_tab(Some(url), window, cx);
        } else {
            cx.open_url(url.as_str());
        }
    }
}

pub(super) struct Painter<'a> {
    pub(super) theme: &'a Theme,
    pub(super) name: &'a str,
    pub(super) size: f32,
    /// The focus ring's color while the place's region holds the keyboard;
    /// controls are tab stops then, and Enter or Space uses one.
    pub(super) ring: Option<Rgba>,
}

impl Painter<'_> {
    /// `control` as a tab stop with a focus ring, while the region holds the
    /// keyboard; as the app's own chrome controls are (`chrome_stop`).
    fn stop(&self, control: Stateful<Div>) -> Stateful<Div> {
        let Some(ring) = self.ring else {
            return control;
        };
        control.tab_index(0).focus_visible(move |style| {
            style.shadow(vec![BoxShadow {
                color: ring.into(),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: true,
            }])
        })
    }

    pub(super) fn node(
        &self,
        node: &Node,
        id: &str,
        axis: Axis,
        cx: &mut Context<HerdrWindow>,
    ) -> AnyElement {
        let theme = self.theme;
        match node {
            Node::Row(container) => self.container(container, id, Axis::Row, cx),
            Node::Column(container) => self.container(container, id, Axis::Column, cx),
            Node::List(list) => div()
                .flex()
                .flex_col()
                .w_full()
                .children(list.items.iter().enumerate().map(|(index, item)| {
                    div()
                        .when(index > 0, |row| {
                            row.border_t_1().border_color(rgb(theme.active))
                        })
                        .py(px(3.))
                        .child(self.node(item, &format!("{id}-{index}"), Axis::Column, cx))
                }))
                .into_any_element(),
            Node::Text(text) => div()
                .text_size(px(self.text_size(text.size)))
                .text_color(tone(theme, text.tone))
                .when(text.weight == Weight::Bold, |el| {
                    el.font_weight(FontWeight::BOLD)
                })
                .when(text.truncate, |el| {
                    el.min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                })
                .child(SharedString::from(text.text.clone()))
                .into_any_element(),
            Node::Icon(icon) => self.icon(&icon.source, tone(theme, icon.tone)),
            Node::Badge(badge) => {
                let color = tone(theme, badge.tone);
                div()
                    .flex_none()
                    .px(px(5.))
                    .rounded(px(crate::config::corners::SMALL))
                    .bg(color.opacity(0.18))
                    .text_color(color)
                    .text_size(px(self.text_size(Size::Small)))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(badge.text.clone()))
                    .into_any_element()
            }
            Node::Button(button) => {
                let color = tone(theme, button.tone);
                let mut element = div()
                    .id(SharedString::from(format!("{id}-button")))
                    .role(Role::Button)
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(4.))
                    .px(px(6.))
                    .rounded(px(crate::config::corners::SMALL))
                    .text_color(if button.disabled {
                        rgb(theme.muted)
                    } else {
                        color
                    })
                    .children(button.icon.as_ref().map(|icon| self.icon(icon, color)))
                    .children(button.label.clone().map(SharedString::from));
                if let Some(label) = &button.label {
                    element = element.aria_label(SharedString::from(label.clone()));
                }
                if !button.disabled {
                    let (name, action) = (self.name.to_owned(), button.action.clone());
                    element = self
                        .stop(element)
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(theme.active)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.mod_action(&name, &action, window, cx);
                        }));
                }
                element.into_any_element()
            }
            Node::Progress(progress) => self.bar(
                progress.value.unwrap_or(1.),
                tone(theme, progress.tone),
                progress.value.is_none(),
            ),
            Node::Meter(meter) => div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .min_w(px(48.))
                .children(meter.label.clone().map(|label| {
                    div()
                        .text_size(px(self.text_size(Size::Small)))
                        .text_color(rgb(theme.muted))
                        .child(SharedString::from(label))
                }))
                .child(self.bar(meter.value, tone(theme, meter.tone), false))
                .into_any_element(),
            Node::Link(link) => {
                let url = link.url.clone();
                self.stop(div().id(SharedString::from(format!("{id}-link"))))
                    .role(Role::Link)
                    .cursor_pointer()
                    .text_color(accent(theme))
                    .hover(|style| style.underline())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_mod_link(&url, window, cx);
                    }))
                    .child(SharedString::from(link.label.clone()))
                    .into_any_element()
            }
            Node::Toggle(toggle) => {
                let (name, msg) = (self.name.to_owned(), toggle.msg.clone());
                let on = toggle.on;
                let track = if on { accent(theme) } else { rgb(theme.active) };
                self.stop(div().id(SharedString::from(format!("{id}-toggle"))))
                    .role(Role::Switch)
                    .aria_toggled(on.into())
                    .aria_label(SharedString::from(toggle.label.clone()))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.send_to_mod(&name, msg.clone(), cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .w(px(22.))
                            .h(px(12.))
                            .p(px(2.))
                            .rounded_full()
                            .bg(track)
                            .when(on, |el| el.justify_end())
                            .child(div().size(px(8.)).rounded_full().bg(rgb(theme.foreground))),
                    )
                    .child(SharedString::from(toggle.label.clone()))
                    .into_any_element()
            }
            Node::Tooltip(tooltip) => {
                let text = SharedString::from(tooltip.text.clone());
                let (foreground, surface) = (theme.foreground, theme.surface);
                div()
                    .id(SharedString::from(format!("{id}-tip")))
                    .tooltip(move |_, cx| {
                        cx.new(|_| crate::usage::Hint {
                            text: text.clone(),
                            foreground,
                            surface,
                        })
                        .into()
                    })
                    .child(self.node(&tooltip.child, &format!("{id}-0"), axis, cx))
                    .into_any_element()
            }
            Node::Spacer => div().flex_1().into_any_element(),
            Node::Divider => match axis {
                Axis::Row => div().w(px(1.)).h(px(self.size)).bg(rgb(theme.active)),
                Axis::Column => div().h(px(1.)).w_full().bg(rgb(theme.active)),
            }
            .into_any_element(),
        }
    }

    fn container(
        &self,
        container: &Container,
        id: &str,
        axis: Axis,
        cx: &mut Context<HerdrWindow>,
    ) -> AnyElement {
        let gap = match container.gap {
            Gap::None => 0.,
            Gap::Small => 4.,
            Gap::Normal => 8.,
            Gap::Large => 14.,
        };
        div()
            .flex()
            .when(axis == Axis::Column, |el| el.flex_col())
            .map(|el| match container.align {
                Align::Stretch => el,
                Align::Start => el.items_start(),
                Align::Center => el.items_center(),
                Align::End => el.items_end(),
            })
            .gap(px(gap))
            .min_w_0()
            .children(container.children.iter().enumerate().map(|(index, child)| {
                let element = self.node(child, &format!("{id}-{index}"), axis, cx);
                // In a row, children keep their natural width so text never
                // wraps a letter at a time; only text asked to truncate, and
                // spacers, give way.
                let yields = matches!(child, Node::Spacer)
                    || matches!(child, Node::Text(text) if text.truncate);
                if axis == Axis::Row && !yields {
                    div().flex_none().child(element).into_any_element()
                } else {
                    element
                }
            }))
            .into_any_element()
    }

    /// Mod-folder SVGs are not drawn yet (they need the image cache the
    /// activity bar's icons will add), so only the app's icons show.
    fn icon(&self, source: &IconSource, color: Rgba) -> AnyElement {
        match source {
            IconSource::App(name) => svg()
                .path(SharedString::from(format!("icons/{name}.svg")))
                .flex_none()
                .size(px(self.size))
                .text_color(color)
                .into_any_element(),
            IconSource::File(_) => div().into_any_element(),
        }
    }

    fn bar(&self, value: f64, color: Rgba, unknown: bool) -> AnyElement {
        div()
            .w_full()
            .min_w(px(40.))
            .h(px(4.))
            .rounded_full()
            .bg(rgb(self.theme.active))
            .child(
                div()
                    .h_full()
                    .rounded_full()
                    .w(relative(value as f32))
                    .bg(if unknown { color.opacity(0.4) } else { color }),
            )
            .into_any_element()
    }

    fn text_size(&self, size: Size) -> f32 {
        match size {
            Size::Small => self.size * 0.85,
            Size::Normal => self.size,
            Size::Large => self.size * 1.25,
        }
    }
}

/// Status tones use the literals the agent dots use, darkened on light
/// themes, so `attention` matches a blocked agent's dot.
fn tone(theme: &Theme, tone: Tone) -> Rgba {
    match tone {
        Tone::Normal => rgb(theme.foreground),
        Tone::Muted => rgb(theme.muted),
        Tone::Accent => accent(theme),
        Tone::Success => rgb(theme.ink(0xa6e3a1)),
        Tone::Warning => rgb(theme.ink(0xf9e2af)),
        Tone::Danger | Tone::Attention => rgb(theme.ink(0xf38ba8)),
    }
}
