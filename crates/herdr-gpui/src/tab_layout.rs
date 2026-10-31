//! Tab strips as swappable layouts.
//!
//! The strip prepares what a tab shows once, as a [`TabFace`], and hands it
//! to the configured [`TabLayout`], which decides the tab's frame, where its
//! mark and close button go, and how being chosen reads, as the sidebar's row
//! layouts do for rows. The strip keeps the tab's identity, accessibility,
//! listeners, growth, and dragging, so a tab behaves the same in every layout.
//!
//! A layout also says how wide the chrome around a title is, from the same
//! constants it draws with: the strip measures tabs before it builds them, to
//! grow, reveal, and slide them.

mod cards;
mod classic;
mod compact;
mod curved;
mod outline;
pub(crate) mod panes;
mod pills;
mod underline;

use crate::{
    backdrop::Fill,
    browser::{Slot, TabId},
    config::{FontConfig, TabLayoutMode, Theme},
    icons::AgentIcon,
    sidebar::{Indicators, status_indicator},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;
use std::sync::Arc;

pub(crate) use classic::{DOT_GAP, ZOOM_ICON};

/// What a tab draws before its title, as far as its width is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lead {
    Nothing,
    /// A Herdr tab whose agents report a status.
    Status,
    /// A browser tab's icon or the page's own.
    Icon,
}

/// What a tab's width depends on besides its title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TabShape {
    pub(crate) lead: Lead,
    pub(crate) zoomed: bool,
    /// The user muted the tab's notifications, or its workspace's.
    pub(crate) muted: bool,
    /// A layout may name only the chosen tab.
    pub(crate) selected: bool,
}

impl TabShape {
    /// How wide the marks after the title draw: Herdr's zoom, then this
    /// client's mute, each `DOT_GAP` after what precedes it.
    pub(crate) fn marks_width(self) -> f32 {
        (usize::from(self.zoomed) + usize::from(self.muted)) as f32 * (DOT_GAP + ZOOM_ICON)
    }
}

/// What leads a tab's title.
pub(crate) enum Mark {
    /// A Herdr tab: its agents' status, and the agent whose logo stands for
    /// it, if any runs in it.
    Herdr {
        status: AgentStatus,
        agent: Option<AgentIcon>,
    },
    /// A browser tab's kind: a globe, a diff, a file, or an editor's mark.
    Icon(&'static str),
    /// The page's own icon, in place of the globe.
    Favicon(Arc<RenderImage>),
}

impl Mark {
    /// Unknown is what a tab without an agent reports, so it leads with
    /// nothing rather than the sidebar's small grey dot.
    pub(crate) fn lead(&self) -> Lead {
        match self {
            Self::Herdr {
                status: AgentStatus::Unknown,
                ..
            } => Lead::Nothing,
            Self::Herdr { .. } => Lead::Status,
            Self::Icon(_) | Self::Favicon(_) => Lead::Icon,
        }
    }
}

/// Which tab a face draws, for the probes tests find its parts by.
#[derive(Clone, Debug)]
pub(crate) enum Probe {
    Herdr(String),
    Page(TabId),
}

impl Probe {
    fn status(&self, slot: Slot) -> impl Fn() -> String + 'static {
        let name = match self {
            Self::Herdr(id) => format!("tab-status-{id}"),
            Self::Page(id) => format!("browser-tab-status-{id}"),
        };
        move || slot.selector(&name)
    }

    fn zoom(&self, slot: Slot) -> impl Fn() -> String + 'static {
        let name = match self {
            Self::Herdr(id) => format!("tab-zoom-{id}"),
            Self::Page(id) => format!("browser-tab-zoom-{id}"),
        };
        move || slot.selector(&name)
    }

    fn muted(&self, slot: Slot) -> impl Fn() -> String + 'static {
        let name = match self {
            Self::Herdr(id) => format!("tab-muted-{id}"),
            Self::Page(id) => format!("browser-tab-muted-{id}"),
        };
        move || slot.selector(&name)
    }

    fn favicon(&self, slot: Slot) -> impl Fn() -> String + 'static {
        let name = match self {
            Self::Herdr(id) => format!("tab-favicon-{id}"),
            Self::Page(id) => format!("favicon-{id}"),
        };
        move || slot.selector(&name)
    }

    /// The hover group a layout names on the tab, so its parts can answer
    /// the pointer anywhere over it.
    fn hover_group(&self) -> SharedString {
        match self {
            Self::Herdr(id) => format!("tab-hover-{id}").into(),
            Self::Page(id) => format!("browser-tab-hover-{id}").into(),
        }
    }

    /// The close button's element ID, and its probe and its icon's.
    fn close(&self, slot: Slot) -> (&'static str, String, String) {
        match self {
            Self::Herdr(id) => (
                "close-tab",
                slot.selector(&format!("close-tab-{id}")),
                slot.selector(&format!("close-tab-icon-{id}")),
            ),
            Self::Page(id) => (
                "close-browser-tab",
                slot.selector(&format!("close-browser-tab-{id}")),
                slot.selector(&format!("close-browser-tab-icon-{id}")),
            ),
        }
    }
}

/// A click listener, erased so Herdr and browser tabs share one type.
pub(crate) type ClickHandler = dyn Fn(&ClickEvent, &mut Window, &mut App);

/// What one tab shows. The strip owns what closing it does.
pub(crate) struct TabFace {
    pub(crate) label: SharedString,
    pub(crate) mark: Mark,
    /// Herdr's zoom: one pane fills the tab, the rest are hidden behind it.
    pub(crate) zoomed: bool,
    /// The user muted the tab's notifications, or its workspace's.
    pub(crate) muted: bool,
    pub(crate) probe: Probe,
    pub(crate) close: Box<ClickHandler>,
}

/// How a tab is marked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TabState {
    /// The tab its group shows.
    pub(crate) selected: bool,
    /// Its group has the keyboard, or the window is not split.
    pub(crate) active: bool,
}

/// Read-only inputs every tab of one strip shares.
pub(crate) struct TabContext<'a> {
    pub(crate) theme: &'a Theme,
    /// The tab font.
    pub(crate) font: &'a FontConfig,
    pub(crate) indicators: Indicators,
    pub(crate) fill: Fill,
    pub(crate) slot: Slot,
    /// The keyboard ring's color while the tab strip takes Tab focus.
    pub(crate) ring: Option<u32>,
}

impl TabContext<'_> {
    /// Whether the strip takes Tab focus. Its close buttons are stops then,
    /// so a layout that keeps them for the pointer shows them anyway: a
    /// stop the keyboard can land on must be seen.
    pub(crate) fn keyboard(&self) -> bool {
        self.ring.is_some()
    }

    /// Gives `control` a stop and a ring while the strip takes Tab focus,
    /// as the window's other chrome does.
    pub(crate) fn stop<E: InteractiveElement>(&self, control: E) -> E {
        let Some(ring) = self.ring else {
            return control;
        };
        control.tab_index(0).focus_visible(move |style| {
            style.shadow(vec![BoxShadow {
                color: rgb(ring).into(),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: true,
            }])
        })
    }
}

/// Arranges a tab's content and marks its state. Implementations are
/// stateless and read only what they are given, so render stays free of I/O.
pub(crate) trait TabLayout {
    /// How wide a tab of `shape` whose shaped title is `title` wide draws,
    /// padding, gaps, marks, and close button included. It must agree with
    /// [`Self::tab`].
    fn width(&self, title: f32, shape: TabShape, font: &FontConfig, indicators: Indicators) -> f32;

    /// The tab's fill and text color. A carried tab's card and the drag
    /// ghost take the same.
    fn colors(&self, state: TabState, theme: &Theme) -> (u32, u32);

    /// The tab, built onto `base`, which already carries its identity,
    /// role, and focus stop. The strip adds listeners after.
    fn tab(
        &self,
        base: Stateful<Div>,
        face: TabFace,
        state: TabState,
        cx: &TabContext<'_>,
    ) -> Stateful<Div>;

    /// Space the strip leaves before its first tab and after its last,
    /// which a layout whose tabs float on the strip needs.
    fn padding(&self) -> f32 {
        0.
    }

    /// The strip's "+" button, built onto `base`, which already carries its
    /// identity, role, and listener, around `icon`: by default a cell like
    /// a classic tab, ruled off from what follows.
    fn new_tab(&self, base: Stateful<Div>, icon: AnyElement, theme: &Theme) -> Stateful<Div> {
        let hover = theme.active;
        base.w(px(self.new_tab_width()))
            .min_h(px(crate::TAB_HEIGHT))
            .border_r_1()
            .border_color(rgb(theme.active))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |s| s.bg(rgb(hover)))
            .child(icon)
    }

    /// How wide [`Self::new_tab`] draws, rule included.
    fn new_tab_width(&self) -> f32 {
        34.
    }

    /// The narrowest a tab draws, which a growing tab starts from.
    fn min_width(&self) -> f32 {
        crate::TAB_WIDTH
    }

    /// The surface joining the chosen tab to what its group shows, for a
    /// layout whose tabs leave that to the strip. The strip then paints one
    /// body behind its tabs, so it can slide between them, and sits the
    /// group's content in a card.
    fn connected(&self) -> Option<Connected> {
        None
    }

    /// How a herdr-core tab split into panes reads under these tabs.
    fn panes(&self) -> &'static dyn panes::PaneChrome;
}

/// The shape of a connected tab body and the card it flows into.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Connected {
    /// How far the body stands below the strip's edge away from the content.
    pub(crate) top: f32,
    /// The body's corners away from the content.
    pub(crate) radius: f32,
    /// How far each concave shoulder flares past the tab at its foot.
    pub(crate) shoulder: f32,
    /// The chrome kept around the content card.
    pub(crate) inset: f32,
    /// The content card's corners.
    pub(crate) card_radius: f32,
}

/// The space floating layouts leave at each end of their tabs.
const FLOATING_PADDING: f32 = 6.;

/// The rounded part of a floating layout's "+", and the room either side.
const FLOATING_NEW_TAB: f32 = 22.;
const FLOATING_NEW_TAB_MARGIN: f32 = 2.;
/// How wide a floating layout's "+" draws.
const FLOATING_NEW_TAB_WIDTH: f32 = FLOATING_NEW_TAB + 2. * FLOATING_NEW_TAB_MARGIN;

/// A floating layout's "+": a small rounded button standing on the strip
/// as its tabs do, with no rule beside it.
fn floating_new_tab(base: Stateful<Div>, icon: AnyElement, theme: &Theme) -> Stateful<Div> {
    const SIZE: f32 = FLOATING_NEW_TAB;
    let hover = theme.active;
    base.flex_none()
        .w(px(FLOATING_NEW_TAB_WIDTH))
        .min_h(px(crate::TAB_HEIGHT))
        .px(px(FLOATING_NEW_TAB_MARGIN))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(
            div()
                .size(px(SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(crate::config::corners::CONTROL))
                .hover(move |s| s.bg(rgb(hover)))
                .child(icon),
        )
}

/// The tabs a layout draws.
pub(crate) fn layout_for(mode: TabLayoutMode) -> &'static dyn TabLayout {
    match mode {
        TabLayoutMode::Cards => &cards::Cards,
        TabLayoutMode::Classic => &classic::Classic,
        TabLayoutMode::Compact => &compact::Compact,
        TabLayoutMode::Curved => &curved::Curved,
        TabLayoutMode::Outline => &outline::Outline,
        TabLayoutMode::Pills => &pills::Pills,
        TabLayoutMode::Underline => &underline::Underline,
    }
}

/// The mark before a title, `size` square, in `text`'s color. A Herdr tab
/// draws its status here; one without a status draws nothing.
fn lead_mark(
    mark: &Mark,
    probe: &Probe,
    size: f32,
    text: u32,
    cx: &TabContext<'_>,
) -> Option<AnyElement> {
    match mark {
        Mark::Herdr { status, .. } => (mark.lead() == Lead::Status).then(|| {
            status_indicator(*status, cx.font, cx.indicators)
                .mt_0()
                .debug_selector(probe.status(cx.slot))
                .into_any_element()
        }),
        Mark::Icon(path) => Some(icon(path, size, text)),
        Mark::Favicon(favicon) => Some(
            img(favicon.clone())
                .debug_selector(probe.favicon(cx.slot))
                .size(px(size))
                .flex_none()
                .into_any_element(),
        ),
    }
}

/// What runs in a tab, `size` square: the agent's logo or a terminal for a
/// Herdr tab, a browser tab's own icon otherwise.
fn kind_icon(mark: &Mark, probe: &Probe, size: f32, text: u32, cx: &TabContext<'_>) -> AnyElement {
    match mark {
        Mark::Herdr { agent, .. } => icon(
            agent.map_or("icons/terminal.svg", |agent| agent.path()),
            size,
            text,
        ),
        Mark::Icon(_) | Mark::Favicon(_) => {
            lead_mark(mark, probe, size, text, cx).unwrap_or_else(|| div().into_any_element())
        }
    }
}

/// A Herdr tab's status, for layouts that lead with what runs in the tab
/// and show the status elsewhere. Browser tabs and tabs without a status
/// have none.
fn status_mark(mark: &Mark, probe: &Probe, cx: &TabContext<'_>) -> Option<AnyElement> {
    match mark {
        Mark::Herdr { .. } => lead_mark(mark, probe, 0., 0, cx),
        Mark::Icon(_) | Mark::Favicon(_) => None,
    }
}

/// How wide a Herdr tab's status draws, when it has one.
fn status_width(lead: Lead, font: &FontConfig, indicators: Indicators) -> Option<f32> {
    (lead == Lead::Status).then(|| indicators.width(font))
}

fn icon(path: &'static str, size: f32, color: u32) -> AnyElement {
    svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(rgb(color))
        .into_any_element()
}

/// The mark after a zoomed tab's title.
fn zoom_mark(zoomed: bool, probe: &Probe, text: u32, cx: &TabContext<'_>) -> Option<Svg> {
    zoomed.then(|| {
        svg()
            .path("icons/zoom.svg")
            .debug_selector(probe.zoom(cx.slot))
            .flex_none()
            .size(px(ZOOM_ICON))
            .text_color(rgb(text))
    })
}

/// The mark after a muted tab's title, beside the zoom mark and as large.
fn mute_mark(muted: bool, probe: &Probe, text: u32, cx: &TabContext<'_>) -> Option<Svg> {
    muted.then(|| {
        svg()
            .path(crate::icons::MUTED)
            .debug_selector(probe.muted(cx.slot))
            .flex_none()
            .size(px(ZOOM_ICON))
            .text_color(rgb(text))
    })
}

/// A tooltip standing in for a tab's title: the title, and the mute its
/// hidden mark would show.
fn hint(label: SharedString, muted: bool) -> SharedString {
    if muted {
        format!("{label} (notifications muted)").into()
    } else {
        label
    }
}

/// The close button, `size` square, whose press never reaches the tab.
fn close_button(
    close: Box<ClickHandler>,
    probe: &Probe,
    size: f32,
    text: u32,
    cx: &TabContext<'_>,
) -> Stateful<Div> {
    let (id, probe, icon_probe) = probe.close(cx.slot);
    cx.stop(div().id(id).role(Role::Button).aria_label("Close tab"))
        .debug_selector(move || probe.clone())
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(crate::config::corners::CONTROL))
        .hover(move |s| s.bg(rgba((text << 8) | 0x24)))
        .child(
            svg()
                .path("icons/close.svg")
                .debug_selector(move || icon_probe.clone())
                .size(px(12.))
                .text_color(rgb(text)),
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            close(event, window, cx);
        })
}

#[cfg(test)]
mod tests;
