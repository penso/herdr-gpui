//! The list's heading, which is also its search field. At rest it reads as
//! the heading, its label standing in the field's place with a magnifier at
//! the edge. Focused or holding a query, it shows what it searches. Its frame
//! is a row's highlight, with the same inset and padding, so the label, the
//! query, and the rows under it all start at the same x.

use crate::{
    HerdrWindow,
    config::corners,
    search_input::SearchInput,
    sidebar::{layout::SidebarLook, line_height},
};
use gpui::{prelude::*, *};

/// Which list a heading searches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::sidebar) enum Heading {
    /// The spaces list, which results replace while there is a query.
    Spaces,
    /// The Devices layout's tree, which a query narrows in place.
    DeviceTree,
}

impl Heading {
    fn label(self) -> &'static str {
        match self {
            Self::Spaces => "spaces",
            Self::DeviceTree => "devices",
        }
    }

    /// The field's accessible name, which stays put while the visible
    /// label swaps between the heading and the placeholder.
    fn name(self) -> &'static str {
        match self {
            Self::Spaces => "Search spaces",
            Self::DeviceTree => "Search devices",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::Spaces => "Devices, worktrees, branches, notes",
            Self::DeviceTree => "Search devices and agents",
        }
    }

    fn selector(self) -> &'static str {
        match self {
            Self::Spaces => "sidebar-search",
            Self::DeviceTree => "device-tree-search",
        }
    }
}

impl HerdrWindow {
    fn heading_input(&self, heading: Heading) -> Option<&Entity<SearchInput>> {
        match heading {
            Heading::Spaces => Some(&self.sidebar_search.input),
            Heading::DeviceTree => self
                .devices_overview
                .tree_search
                .as_ref()
                .map(|search| &search.input),
        }
    }

    /// The heading for `heading`'s list, or `None` while the Devices layout's
    /// field has yet to be made.
    pub(in crate::sidebar) fn search_heading(
        &self,
        heading: Heading,
        look: SidebarLook,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let input = self.heading_input(heading)?.clone();
        let theme = &self.theme;
        let font = &self.config.sidebar;
        let (focus, typed) = {
            let input = input.read(cx);
            (input.focus.clone(), !input.text().is_empty())
        };
        let open = typed || focus.is_focused(window);
        let placeholder = if open {
            heading.placeholder().to_owned()
        } else {
            look.header_label(heading.label())
        };
        input.update(cx, |input, cx| {
            input.set_label(heading.name(), cx);
            input.set_placeholder(&placeholder, cx);
        });
        let magnifier = || {
            svg()
                .path("icons/search.svg")
                .size(px(12.))
                .flex_none()
                .text_color(rgb(theme.muted))
        };
        // The frame's border sits inside the inset, so the padding gives up
        // its pixel and text lands where the rows' does.
        let padding = (look.density.padding() - 1.).max(0.);
        let field = div()
            .id(heading.selector())
            .debug_selector(|| heading.selector().into())
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(px(look.density.gap()))
            .px(px(padding))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx);
            })
            .on_key_down(cx.listener(move |this, event, window, cx| match heading {
                Heading::Spaces => this.sidebar_search_key(event, window, cx),
                Heading::DeviceTree => this.device_tree_key(event, window, cx),
            }))
            .when(open, |field| field.child(magnifier()))
            .child(
                div()
                    .debug_selector(move || format!("header-label-{}", heading.label()))
                    .flex_1()
                    .min_w_0()
                    .child(input),
            )
            .when(!open, |field| field.child(magnifier()))
            .when(typed, |field| {
                field.child(
                    div()
                        .id("sidebar-search-clear")
                        .debug_selector(|| "sidebar-search-clear".into())
                        .role(Role::Button)
                        .aria_label("Clear search")
                        .flex_none()
                        .size(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(corners::SMALL))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(theme.active)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.clear_heading_search(heading, window, cx);
                        }))
                        .child(
                            svg()
                                .path("icons/x.svg")
                                .size(px(12.))
                                .text_color(rgb(theme.muted)),
                        ),
                )
            });
        Some(
            div()
                .debug_selector(move || format!("header-{}", heading.label()))
                .flex_none()
                .h(px(line_height(font) + 2. * look.density.header_padding()))
                .px(px(look.inset()))
                .flex()
                .items_center()
                .child(look.field_frame(field, open, theme)),
        )
    }

    /// Empties `heading`'s field and gives the keyboard back to the terminal.
    pub(in crate::sidebar) fn clear_heading_search(
        &mut self,
        heading: Heading,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = self.heading_input(heading) {
            input.update(cx, |input, cx| input.clear(cx));
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The tree narrows as the field changes, so only Escape needs handling:
    /// it clears the field and returns to the heading.
    fn device_tree_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let composing = self
            .heading_input(Heading::DeviceTree)
            .is_some_and(|input| input.read(cx).is_composing());
        if composing || event.keystroke.key != "escape" {
            return;
        }
        self.clear_heading_search(Heading::DeviceTree, window, cx);
        cx.stop_propagation();
    }
}
