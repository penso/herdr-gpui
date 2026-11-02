//! Mod apps in the activity bar: a mod whose manifest has `[activity]` gets
//! an icon after the built-in sections, and picking it gives the mod's tree
//! the window's body, as a section does. The icon is the mod's own SVG with
//! `currentColor` set to the bar's ink, so it tints like the built-in icons.

use super::render::{Axis, Painter};
use crate::{
    HerdrWindow,
    sidebar::{BAR_BUTTON, BAR_ICON},
};
use gpui::{prelude::*, *};
use std::sync::Arc;

/// The widest a mod app's content gets, so a wide window keeps it readable.
const APP_WIDTH: f32 = 880.;

impl HerdrWindow {
    /// The mod app shown across the body: one picked from the bar whose mod
    /// still runs and still declares an app.
    pub(crate) fn mod_app(&self) -> Option<&str> {
        let name = self.activity.app.as_deref()?;
        let offered = self
            .mods
            .get(name)
            .is_some_and(|entry| entry.activity().is_some());
        (self.activity_bar_leads() && offered).then_some(name)
    }

    /// Stops showing a mod app, back to the section under it, and gives
    /// the keyboard back to what now shows: the terminal under Workspaces,
    /// or that section. Picking the app moved it to the section's handle,
    /// which nothing else would move back.
    pub(crate) fn leave_mod_app(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activity.app.take().is_none() {
            return;
        }
        let focus = match self.activity_section() {
            None => Some(self.focus.clone()),
            Some(_) => self.activity.section_focus.clone(),
        };
        if let Some(focus) = focus {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub(crate) fn select_mod_app(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activity.app = Some(name.to_owned());
        // The mod's body hides the terminals, so typing must not reach them.
        let focus = self
            .activity
            .section_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// A bar button per mod that declares an app, in name order.
    pub(crate) fn mod_activity_buttons(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = &self.theme;
        let shown = self.mod_app();
        self.mods
            .entries()
            .filter_map(|(name, entry)| Some((name, entry, entry.activity()?)))
            .map(|(name, entry, activity)| {
                let selected = shown == Some(name);
                let ink = if selected {
                    theme.foreground
                } else {
                    theme.muted
                };
                let glyph = match entry.tinted_icon(ink) {
                    Some(image) => img(image).size(px(BAR_ICON)).flex_none().into_any_element(),
                    None => svg()
                        .path("icons/settings.svg")
                        .size(px(BAR_ICON))
                        .text_color(rgb(ink))
                        .into_any_element(),
                };
                let title = SharedString::from(activity.title.clone());
                let (foreground, surface, hover) = (theme.foreground, theme.surface, theme.active);
                let owned = name.to_owned();
                div()
                    .id(SharedString::from(format!("activity-mod-{name}")))
                    .debug_selector(|| format!("activity-mod-{name}"))
                    .role(Role::Tab)
                    .aria_selected(selected)
                    .aria_label(title.clone())
                    .relative()
                    .size(px(BAR_BUTTON))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(crate::config::corners::CONTROL))
                    .tooltip(move |_, cx| {
                        cx.new(|_| crate::menu::SettingsHint {
                            text: title.clone(),
                            foreground,
                            surface,
                        })
                        .into()
                    })
                    .when(selected, |button| {
                        button.bg(rgb(theme.primary_wash())).child(
                            div()
                                .absolute()
                                .left(px(-6.))
                                .top(px(6.))
                                .bottom(px(6.))
                                .w(px(2.))
                                .rounded_full()
                                .bg(rgb(theme.primary())),
                        )
                    })
                    .when(!selected, |button| {
                        button
                            .cursor_pointer()
                            .hover(move |style| style.bg(rgb(hover)))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_mod_app(&owned, window, cx);
                    }))
                    .child(glyph)
                    .into_any_element()
            })
            .collect()
    }

    /// The shown mod app across the body: its title, its last error, and
    /// its tree.
    pub(crate) fn render_mod_app(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let name = self.mod_app()?;
        let entry = self.mods.get(name)?;
        let title = entry.activity()?.title.clone();
        let theme = &self.theme;
        let size = self.config.ui.size;
        let painter = Painter {
            theme,
            name,
            size,
            ring: None,
        };
        let body = match &entry.tree {
            Some(tree) => painter.node(tree, &format!("mod-app-{name}"), Axis::Column, cx),
            // Its view returned nil: done, with nothing to show.
            None if entry.drawn => div().into_any_element(),
            None => div()
                .debug_selector(|| "mod-app-loading".into())
                .text_color(rgb(theme.muted))
                .child("Loading…")
                .into_any_element(),
        };
        let error = entry.error().map(|error| {
            div()
                .px(px(10.))
                .py(px(6.))
                .rounded(px(crate::config::corners::CONTROL))
                .bg(rgb(theme.ink(0xf38ba8)).opacity(0.15))
                .text_color(rgb(theme.ink(0xf38ba8)))
                .child(SharedString::from(error.to_string()))
        });
        let focus = self.activity.section_focus.clone();
        Some(
            div()
                .id(SharedString::from(format!("mod-app-{name}")))
                .debug_selector(|| format!("mod-app-{name}"))
                .when_some(focus, |app, focus| app.track_focus(&focus))
                .flex()
                .flex_col()
                .flex_1()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_y_scroll()
                .bg(rgb(theme.background))
                .text_color(rgb(theme.foreground))
                .text_size(px(size))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(14.))
                        .w_full()
                        .max_w(px(APP_WIDTH))
                        .px(px(28.))
                        .py(px(22.))
                        .child(
                            div()
                                .text_size(px(size * 1.6))
                                .font_weight(FontWeight::BOLD)
                                .child(SharedString::from(title)),
                        )
                        .children(error)
                        .child(body),
                )
                .into_any_element(),
        )
    }
}

impl super::Entry {
    /// The icon drawn in `ink`, made once per color and icon.
    fn tinted_icon(&self, ink: u32) -> Option<Arc<Image>> {
        let svg = self.icon.as_deref()?;
        let mut cache = self.icon_image.borrow_mut();
        if let Some((color, image)) = cache.as_ref()
            && *color == ink
        {
            return Some(Arc::clone(image));
        }
        let image = Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            tint(svg, ink).into_bytes(),
        ));
        *cache = Some((ink, Arc::clone(&image)));
        Some(image)
    }
}

/// The SVG with `currentColor` replaced by `ink`, as `#rrggbb`.
pub(super) fn tint(svg: &str, ink: u32) -> String {
    svg.replace("currentColor", &format!("#{:06x}", ink & 0xff_ff_ff))
}
