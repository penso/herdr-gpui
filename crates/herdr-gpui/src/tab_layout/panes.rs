//! How a split tab's panes read in each tab layout. A layout hands out its
//! [`PaneChrome`] beside its tabs, so a herdr-core tab split into panes looks
//! like part of the strip above it: the gutter between panes, a pane's frame
//! and title, and how the pane in use stands out. A tab with one pane shows
//! no chrome at all.

mod classic;
mod curved;
mod pills;
mod underline;

pub(crate) use classic::Classic;
pub(crate) use curved::Curved;
pub(crate) use pills::Pills;
pub(crate) use underline::Underline;

use crate::{
    config::{FontConfig, Theme},
    fonts::StyledFont,
    i18n::fl,
    sidebar::{Indicators, status_indicator},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;

/// What one pane shows around its cells.
pub(crate) struct PaneFace {
    /// The program's title, or the terminal's name.
    pub(crate) title: SharedString,
    /// The folder it runs in, when herdr-core reported one.
    pub(crate) folder: Option<SharedString>,
    pub(crate) status: AgentStatus,
    /// Its size in cells, as `62×24`.
    pub(crate) size: Option<SharedString>,
    /// The tab's pane in use: the one keys go to when the tab has focus.
    pub(crate) active: bool,
}

/// Read-only inputs every pane of one tab shares.
pub(crate) struct PaneContext<'a> {
    pub(crate) theme: &'a Theme,
    /// The tab font, for titles.
    pub(crate) tabs: &'a FontConfig,
    pub(crate) terminal: &'a FontConfig,
    pub(crate) indicators: Indicators,
}

impl PaneContext<'_> {
    /// The status dot the sidebar and tabs draw for `status`.
    fn dot(&self, status: AgentStatus) -> Div {
        status_indicator(status, self.tabs, self.indicators).flex_none()
    }

    /// The word for `status`, for chrome that spells it out; nothing for a
    /// terminal no agent reports on.
    fn state(&self, status: AgentStatus) -> Option<String> {
        match status {
            AgentStatus::Blocked => Some(fl!("core-terminals-state-blocked")),
            AgentStatus::Done => Some(fl!("core-terminals-state-done")),
            AgentStatus::Idle => Some(fl!("core-terminals-state-idle")),
            AgentStatus::Working => Some(fl!("core-terminals-state-working")),
            AgentStatus::Unknown => None,
        }
    }

    fn title_text(&self) -> Div {
        div()
            .text_font(self.tabs)
            .text_size(px(self.tabs.size - 1.))
    }
}

/// Arranges split panes for one tab layout. Implementations are stateless
/// and read only what they are given, so render stays free of I/O.
pub(crate) trait PaneChrome {
    /// Space between panes.
    fn gutter(&self) -> f32;

    /// What shows in the gutter and around an inset split.
    fn gutter_fill(&self, theme: &Theme) -> u32 {
        theme.active
    }

    /// The chrome kept around the whole split.
    fn inset(&self) -> f32 {
        0.
    }

    /// How opaque the veil over a pane not in use is, out of 255.
    fn veil(&self) -> u8;

    /// The cells' own top and bottom corners. A rounded pane rounds what it
    /// holds too: GPUI clips children to a rectangle, so a square fill would
    /// cover the pane's curve.
    fn corners(&self) -> (f32, f32) {
        (0., 0.)
    }

    /// One pane around `cells`, which fill what the pane leaves them.
    fn pane(&self, face: PaneFace, cells: AnyElement, cx: &PaneContext<'_>) -> AnyElement;
}

/// The column every chrome builds a pane in.
fn column() -> Div {
    div().flex().flex_col().flex_1().min_w_0().min_h_0()
}
