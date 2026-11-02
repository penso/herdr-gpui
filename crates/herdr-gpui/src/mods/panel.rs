//! The mod panel: a column on the window's right, beside the terminals,
//! holding a section per mod placed there, as opencode's sidebar holds its
//! plugins' sections. Sections are ordered by `[panel] order`, then name;
//! each header folds its section, kept here so folding never runs Lua; and a
//! mod whose view returns nil has no section. The panel shows while any mod
//! has a section and the user has not hidden it (Toggle Mod Panel), slides in
//! and out, and resizes from its left edge like the notes panel.

use super::render::{Axis, Painter};
use crate::{HerdrWindow, motion, panel_resize::PanelDrag, window::chrome_focus::Region};
use gpui::{prelude::*, *};
use herdr_mods::Place;
use std::collections::BTreeSet;

/// The least room the terminals keep beside the panel; past this the panel
/// narrows, and below [`MIN_PANEL`] it steps aside.
const MIN_CONTENT: f32 = 240.;
/// The narrowest the panel is drawn.
const MIN_PANEL: f32 = 160.;

/// Whether the panel shows, and which sections are folded.
#[derive(Debug, Default)]
pub(super) struct PanelState {
    /// Hidden with Toggle Mod Panel, for this window and session.
    hidden: bool,
    /// Sections folded to their header, by mod name.
    folded: BTreeSet<String>,
    motion: motion::Toggle,
    /// Whether the motion was set yet: the first setting does not slide.
    placed: bool,
}

impl super::Mods {
    pub(crate) fn panel_hidden(&self) -> bool {
        self.panel.hidden
    }

    pub(crate) fn set_panel_hidden(&mut self, hidden: bool) {
        self.panel.hidden = hidden;
    }

    fn toggle_fold(&mut self, name: &str) {
        if !self.panel.folded.remove(name) {
            self.panel.folded.insert(name.to_owned());
        }
    }

    /// The mods with a section, in the panel's order.
    pub(crate) fn panel_sections(&self) -> Vec<(&str, &super::Entry)> {
        let mut sections: Vec<_> = self
            .entries()
            .filter(|(_, entry)| entry.places().contains(&Place::Panel) && entry.tree.is_some())
            .collect();
        sections.sort_by_key(|(name, entry)| {
            let order = entry
                .manifest
                .as_ref()
                .map_or(0, |manifest| manifest.panel.order);
            (order, *name)
        });
        sections
    }
}

impl HerdrWindow {
    /// Sets the panel opening or closing to match its sections and the
    /// user's choice; whether it is moving, so the window draws again.
    pub(crate) fn update_mod_panel(&mut self) -> bool {
        let open = !self.mods.panel.hidden && !self.mods.panel_sections().is_empty();
        let now = motion::now();
        let first = !std::mem::replace(&mut self.mods.panel.placed, true);
        let was = self.mods.panel.motion.is_open();
        self.mods.panel.motion.set(open, now, first);
        was != open || self.mods.panel.motion.moving(now)
    }

    /// The panel's width in this window: its chosen or default width,
    /// capped by the room beside the sidebar so the terminals keep
    /// [`MIN_CONTENT`]; `None` when that leaves less than [`MIN_PANEL`].
    fn mod_panel_width_now(&self) -> Option<f32> {
        let beside =
            self.viewport_width - self.sidebar_column_width(self.viewport_width).unwrap_or(0.);
        let width = self.mod_panel_width.width(beside).min(beside - MIN_CONTENT);
        (width >= MIN_PANEL).then_some(width)
    }

    /// Whether the panel is drawn now, opening or open, for the focus
    /// regions; only beside the terminals, never over a section or app.
    pub(crate) fn mod_panel_shown(&self) -> bool {
        self.mods.panel.motion.is_open()
            && !self.zen_layout()
            && self.activity_section().is_none()
            && self.mod_app().is_none()
            && self.mod_panel_width_now().is_some()
    }

    /// `content` with the panel beside it on the right, while it shows.
    /// Zen mode keeps the terminals alone, as it does with the sidebar.
    pub(crate) fn with_mod_panel(
        &self,
        content: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = motion::now();
        let shown = self.mods.panel.motion.shown(now);
        if shown <= 0. || self.zen_layout() {
            return content;
        }
        let Some(width) = self.mod_panel_width_now() else {
            return content;
        };
        if self.mods.panel.motion.moving(now) {
            window.request_animation_frame();
        }
        let panel = self
            .resizable_panel(
                self.render_mod_panel(cx),
                "mod-panel-resize",
                PanelDrag::ModPanel,
                None,
                cx,
            )
            // Narrowed to the room beside the sidebar, past the drag's cap.
            .w(px(width));
        // Clipped to the share that has slid in, revealed from its left edge.
        let slide = div()
            .relative()
            .flex_none()
            .h_full()
            .w(px(width * shown))
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(width))
                    .child(panel),
            );
        div()
            .flex()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(div().flex().flex_1().min_w_0().min_h_0().child(content))
            .child(slide)
            .into_any_element()
    }

    fn render_mod_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        let size = self.config.ui.size;
        let sections: Vec<AnyElement> = self
            .mods
            .panel_sections()
            .into_iter()
            .filter_map(|(name, entry)| {
                let tree = entry.tree.as_ref()?;
                let manifest = entry.manifest.as_ref()?;
                let title = manifest
                    .panel
                    .title
                    .clone()
                    .unwrap_or_else(|| name.to_owned());
                let folded = self.mods.panel.folded.contains(name);
                let owned = name.to_owned();
                let header = self
                    .chrome_stop(
                        Region::ModPanel,
                        div().id(SharedString::from(format!("mod-panel-{name}-header"))),
                    )
                    .debug_selector(|| format!("mod-panel-{name}-header"))
                    .role(Role::Button)
                    .aria_expanded(!folded)
                    .aria_label(SharedString::from(title.clone()))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_color(rgb(theme.foreground))
                    .font_weight(FontWeight::SEMIBOLD)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mods.toggle_fold(&owned);
                        cx.notify();
                    }))
                    .child(
                        svg()
                            .path(if folded {
                                "icons/chevron-right.svg"
                            } else {
                                "icons/chevron-down.svg"
                            })
                            .size(px(size))
                            .flex_none()
                            .text_color(rgb(theme.muted)),
                    )
                    .child(SharedString::from(title));
                let body = (!folded).then(|| {
                    let painter = Painter {
                        theme,
                        name,
                        size,
                        ring: self.mod_ring(Region::ModPanel),
                    };
                    div()
                        .debug_selector(|| format!("mod-panel-{name}-body"))
                        .child(painter.node(tree, &format!("mod-panel-{name}"), Axis::Column, cx))
                });
                Some(
                    div()
                        .id(SharedString::from(format!("mod-panel-{name}")))
                        .debug_selector(|| format!("mod-panel-{name}"))
                        .flex()
                        .flex_col()
                        .flex_none()
                        .gap(px(6.))
                        .px(px(12.))
                        .py(px(10.))
                        .border_b_1()
                        .border_color(rgb(theme.active))
                        .child(header)
                        .children(body)
                        .into_any_element(),
                )
            })
            .collect();
        div()
            .id("mod-panel")
            .debug_selector(|| "mod-panel".into())
            .role(Role::Complementary)
            .aria_label("Mods")
            .flex()
            .flex_col()
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .bg(self.fill().panel(theme.sidebar_background()))
            .border_l_1()
            .border_color(rgb(theme.active))
            .text_size(px(size))
            .text_color(rgb(theme.foreground))
            .children(sections)
    }
}
