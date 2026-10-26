//! Line icons drawn inline, in the text color, so they match one another
//! on every platform; Unicode symbols such as ⚙ turn into color emoji on
//! iOS.

use maud::{Markup, PreEscaped, html};

/// A 24-unit line icon `size` pixels square, hidden from screen readers:
/// the control around it carries the label.
fn icon(size: u16, body: &str) -> Markup {
    html! {
        svg class="icon" width=(size) height=(size) viewBox="0 0 24 24" fill="none" stroke="currentColor"
            stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            (PreEscaped(body))
        }
    }
}

/// The agents' logos, the files Herdr UI draws (`assets/icons`). Each is
/// one shape in `currentColor`; `.agent-mark svg` sizes it to its tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Agent {
    Claude,
    Codex,
    Copilot,
    Gemini,
    Generic,
    OpenCode,
    Pi,
}

impl Agent {
    pub(crate) fn svg(self) -> Markup {
        PreEscaped(
            match self {
                Self::Claude => include_str!("../../../../../assets/icons/agent-claude.svg"),
                Self::Codex => include_str!("../../../../../assets/icons/agent-codex.svg"),
                Self::Copilot => include_str!("../../../../../assets/icons/agent-copilot.svg"),
                Self::Gemini => include_str!("../../../../../assets/icons/agent-gemini.svg"),
                Self::Generic => include_str!("../../../../../assets/icons/agent-generic.svg"),
                Self::OpenCode => include_str!("../../../../../assets/icons/agent-opencode.svg"),
                Self::Pi => include_str!("../../../../../assets/icons/agent-pi.svg"),
            }
            .trim()
            .to_owned(),
        )
    }
}

pub(crate) fn settings(size: u16) -> Markup {
    icon(
        size,
        r#"<path d="M18.89 9.91L21.28 9.96L21.28 14.04L18.89 14.09L18.35 15.39L20 17.12L17.12 20L15.39 18.35L14.09 18.89L14.04 21.28L9.96 21.28L9.91 18.89L8.61 18.35L6.88 20L4 17.12L5.65 15.39L5.11 14.09L2.72 14.04L2.72 9.96L5.11 9.91L5.65 8.61L4 6.88L6.88 4L8.61 5.65L9.91 5.11L9.96 2.72L14.04 2.72L14.09 5.11L15.39 5.65L17.12 4L20 6.88L18.35 8.61Z"/><circle cx="12" cy="12" r="3"/>"#,
    )
}

pub(crate) fn plus(size: u16) -> Markup {
    icon(size, r#"<path d="M12 5v14M5 12h14"/>"#)
}

pub(crate) fn arrow_up(size: u16) -> Markup {
    icon(size, r#"<path d="M12 19V5M5 12l7-7 7 7"/>"#)
}

pub(crate) fn check(size: u16) -> Markup {
    icon(size, r#"<path d="M5 12.5l4.5 4.5L19 7.5"/>"#)
}

pub(crate) fn stop(size: u16) -> Markup {
    icon(
        size,
        r#"<rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor"/>"#,
    )
}

pub(crate) fn plus_circle(size: u16) -> Markup {
    icon(
        size,
        r#"<circle cx="12" cy="12" r="9"/><path d="M12 8v8M8 12h8"/>"#,
    )
}

/// The board's eye: what needs you.
pub(crate) fn target(size: u16) -> Markup {
    icon(
        size,
        r#"<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="3.5" fill="currentColor"/>"#,
    )
}

pub(crate) fn chevron_down(size: u16) -> Markup {
    icon(size, r#"<path d="M6 9l6 6 6-6"/>"#)
}

pub(crate) fn chevron_right(size: u16) -> Markup {
    icon(size, r#"<path d="M9 6l6 6-6 6"/>"#)
}

/// This computer, beside its name on the home screen.
pub(crate) fn computer(size: u16) -> Markup {
    icon(
        size,
        r#"<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>"#,
    )
}

pub(crate) fn chevron_left(size: u16) -> Markup {
    icon(size, r#"<path d="M15 6l-6 6 6 6"/>"#)
}

pub(crate) fn split_right(size: u16) -> Markup {
    icon(
        size,
        r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/>"#,
    )
}

pub(crate) fn split_down(size: u16) -> Markup {
    icon(
        size,
        r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 12h18"/>"#,
    )
}

pub(crate) fn zoom(size: u16) -> Markup {
    icon(
        size,
        r#"<path d="M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7"/>"#,
    )
}

pub(crate) fn close(size: u16) -> Markup {
    icon(size, r#"<path d="M6 6l12 12M18 6L6 18"/>"#)
}

/// A chat bubble: a new chat.
pub(crate) fn chat(size: u16) -> Markup {
    icon(size, r#"<path d="M4 5h16v11H9l-5 4z"/>"#)
}

/// A clipboard: paste.
pub(crate) fn clipboard(size: u16) -> Markup {
    icon(
        size,
        r#"<rect x="6" y="4" width="12" height="17" rx="2"/><path d="M9 4h6v3H9z"/>"#,
    )
}

/// A landscape in a frame: a picture.
pub(crate) fn picture(size: u16) -> Markup {
    icon(
        size,
        r#"<rect x="3" y="5" width="18" height="14" rx="2"/><circle cx="9" cy="10" r="1.5"/><path d="M21 16l-5-5-9 8"/>"#,
    )
}
