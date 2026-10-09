//! The orchestrator's small drawing parts, in the window's theme and fonts:
//! badges, chips, buttons, status dots. Every color comes from the theme.

use super::rows::Status;
use crate::{
    config::{FontConfig, Theme, corners, mix},
    fonts::StyledFont,
};
use gpui::{prelude::*, *};

pub(crate) const RED: usize = 1;
pub(crate) const GREEN: usize = 2;
pub(crate) const YELLOW: usize = 3;
pub(crate) const BLUE: usize = 4;
pub(crate) const MAGENTA: usize = 5;
pub(crate) const CYAN: usize = 6;

/// What the view draws with; the window replaces it when settings change.
#[derive(Clone, Debug)]
pub(crate) struct Look {
    pub(crate) theme: Theme,
    pub(crate) ui: FontConfig,
    pub(crate) mono: FontConfig,
}

impl Look {
    pub(crate) fn hue(&self, index: usize) -> u32 {
        self.theme.ink(self.theme.palette[index])
    }

    pub(crate) fn wash(&self, index: usize) -> u32 {
        mix(self.theme.background, self.hue(index), 16)
    }

    pub(crate) fn size(&self) -> Pixels {
        px(self.ui.size)
    }

    pub(crate) fn small(&self) -> Pixels {
        px(self.ui.size - 1.)
    }

    pub(crate) fn tiny(&self) -> Pixels {
        px((self.ui.size - 2.).max(9.))
    }

    pub(crate) fn status_hue(&self, status: Status) -> u32 {
        self.hue(match status {
            Status::Starting | Status::Idle => BLUE,
            Status::Working => GREEN,
            Status::NeedsInput => YELLOW,
            Status::Done => CYAN,
            Status::Failed => RED,
            Status::Stopped => return self.theme.muted,
        })
    }

    pub(crate) fn icon(&self, path: &'static str, size: f32, color: u32) -> Svg {
        svg()
            .flex_none()
            .size(px(size))
            .path(path)
            .text_color(rgb(color))
    }

    pub(crate) fn dot(&self, color: u32, size: f32) -> Div {
        div()
            .flex_none()
            .size(px(size))
            .rounded_full()
            .bg(rgb(color))
    }

    /// A short colored word, such as a state or priority.
    pub(crate) fn badge(&self, text: impl Into<SharedString>, index: usize) -> Div {
        div()
            .flex_none()
            .whitespace_nowrap()
            .px(px(6.))
            .rounded(px(corners::SMALL))
            .bg(rgb(self.wash(index)))
            .text_color(rgb(self.hue(index)))
            .text_size(self.tiny())
            .child(text.into())
    }

    /// A neutral tag, such as a label or host.
    pub(crate) fn chip(&self, text: impl Into<SharedString>) -> Div {
        div()
            .flex_none()
            .whitespace_nowrap()
            .px(px(6.))
            .py(px(1.))
            .rounded(px(corners::SMALL))
            .bg(rgb(self.theme.active))
            .text_color(rgb(self.theme.subtext()))
            .text_size(self.small())
            .child(text.into())
    }

    pub(crate) fn mono(&self, text: impl Into<SharedString>) -> Div {
        div()
            .text_font(&self.mono)
            .text_size(self.small())
            .child(text.into())
    }

    pub(crate) fn muted(&self, text: impl Into<SharedString>) -> Div {
        div()
            .text_color(rgb(self.theme.muted))
            .text_size(self.small())
            .child(text.into())
    }

    pub(crate) fn label(&self, text: impl Into<SharedString>) -> Div {
        div()
            .text_size(self.tiny())
            .text_color(rgb(self.theme.muted))
            .font_weight(FontWeight::SEMIBOLD)
            .child(text.into())
    }

    /// A clickable button; `primary` fills it with the theme's accent.
    pub(crate) fn button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        primary: bool,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let fill = if primary {
            theme.primary()
        } else {
            theme.surface
        };
        let hover = mix(fill, theme.foreground, 8);
        div()
            .id(id)
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .px_3()
            .py(px(4.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(fill))
            .hover(move |style| style.bg(rgb(hover)))
            .cursor_pointer()
            .when(!primary, |el| el.border_1().border_color(rgb(theme.active)))
            .text_color(rgb(if primary {
                theme.text_on(fill)
            } else {
                theme.foreground
            }))
            .text_size(self.small())
            .whitespace_nowrap()
            .child(label.into())
    }

    pub(crate) fn icon_button(
        &self,
        id: impl Into<ElementId>,
        path: &'static str,
    ) -> Stateful<Div> {
        let hover = self.theme.active;
        div()
            .id(id)
            .flex_none()
            .size(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(corners::SMALL))
            .hover(move |style| style.bg(rgb(hover)))
            .cursor_pointer()
            .child(self.icon(path, 14., self.theme.subtext()))
    }

    /// A filter pill: a quiet fill, the accent wash while on.
    pub(crate) fn toggle(
        &self,
        id: impl Into<ElementId>,
        label: &'static str,
        on: bool,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(id)
            .flex_none()
            .px_3()
            .py(px(3.))
            .rounded_full()
            .bg(rgb(if on {
                theme.primary_wash()
            } else {
                theme.surface
            }))
            .cursor_pointer()
            .text_size(self.small())
            .text_color(rgb(if on {
                theme.foreground
            } else {
                theme.subtext()
            }))
            .whitespace_nowrap()
            .child(label)
    }

    /// The color of an item's state word.
    pub(crate) fn state_hue(&self, state: &str) -> usize {
        match state {
            "open" => GREEN,
            "in_progress" | "in progress" | "draft" => BLUE,
            "blocked" => YELLOW,
            "merged" => MAGENTA,
            "closed" | "tombstone" => RED,
            _ => CYAN,
        }
    }

    /// The color of a Beads priority, P0 hottest.
    pub(crate) fn priority_hue(&self, priority: i64) -> usize {
        match priority {
            0 | 1 => RED,
            2 => YELLOW,
            _ => BLUE,
        }
    }
}

/// The icon of an agent by the harness name runs record.
pub(crate) fn agent_icon(agent: &str) -> &'static str {
    crate::icons::AgentIcon::from_identity(Some(agent)).path()
}

/// "2d", "5h", "12m": how long ago `at` was.
pub(crate) fn age(
    at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let Some(at) = at else {
        return String::new();
    };
    let seconds = (now - at).num_seconds().max(0);
    match seconds {
        0..60 => "now".into(),
        60..3_600 => format!("{}m", seconds / 60),
        3_600..86_400 => format!("{}h", seconds / 3_600),
        86_400..1_209_600 => format!("{}d", seconds / 86_400),
        _ => format!("{}w", seconds / 604_800),
    }
}
