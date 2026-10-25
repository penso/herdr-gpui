//! HTML fragments the web app shows, rendered from the hub's state with
//! maud, which escapes every interpolated string. Agent text is Markdown
//! rendered with raw HTML turned into text, so nothing a chat says becomes
//! markup. Fragments carry no secrets: they are fetched with a bearer token
//! and swapped into the app shell by id.

mod ansi;
mod chat;
mod home;
mod icons;
mod pages;
mod picker;
mod session;
mod status;
mod terminal;

pub(crate) use ansi::screen;
pub(crate) use chat::{chat, chat_history, chat_patches};
pub(crate) use home::{Lists, board, home, no_match, search, urlencode};
pub(crate) use pages::{Folders, devices, new_chat, new_workspace};
pub(crate) use status::status_mark;
pub(crate) use terminal::{terminal, terminal_screen};

use herdr_chat_protocol::{Status, display};
use maud::{Markup, PreEscaped, html};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd, html::push_html};

/// Markdown as HTML, with raw HTML shown as text, images as links, and
/// links made inert (`rel=noopener`, opened outside the app).
pub(crate) fn markdown(text: &str) -> Markup {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    // An image would load from wherever the agent's text points, telling
    // that server the phone viewed the chat; it becomes a link instead.
    let events = Parser::new_ext(text, options).map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        Event::Start(Tag::Image {
            dest_url, title, ..
        }) => Event::Start(Tag::Link {
            link_type: LinkType::Inline,
            dest_url,
            title,
            id: "".into(),
        }),
        Event::End(TagEnd::Image) => Event::End(TagEnd::Link),
        event => event,
    });
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    push_html(&mut out, events);
    // pulldown-cmark escapes text and attribute values; only link targets
    // could carry a scheme, and those are limited here.
    PreEscaped(neutralize_links(&out))
}

/// Keeps `http(s):` and `mailto:` links; any other scheme (`javascript:`,
/// `data:`) loses its target.
fn neutralize_links(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(index) = rest.find("href=\"") {
        let (before, after) = rest.split_at(index + 6);
        out.push_str(before);
        let end = after.find('"').unwrap_or(after.len());
        let target = &after[..end];
        let lower = target.to_ascii_lowercase();
        let safe = lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
            || lower.starts_with('#');
        out.push_str(if safe { target } else { "#" });
        rest = &after[end..];
    }
    out.push_str(rest);
    out.replace(
        "<a href=",
        "<a rel=\"noopener noreferrer\" target=\"_blank\" href=",
    )
}

/// The square icon for an agent: its logo, the one Herdr UI draws, on
/// its colour.
pub(crate) fn agent_icon(agent: Option<&str>, size: &str) -> Markup {
    let name = agent.unwrap_or_default().to_ascii_lowercase();
    // Copilot before Pi: "copilot" holds "pi".
    let (mark, class) = match name.as_str() {
        n if n.contains("claude") => (icons::Agent::Claude, "bg-claude text-white"),
        n if n.contains("codex") => (icons::Agent::Codex, "bg-codex text-white ring-1 ring-line"),
        n if n.contains("gemini") => (icons::Agent::Gemini, "bg-gemini text-white"),
        n if n.contains("copilot") => (icons::Agent::Copilot, "bg-copilot text-white"),
        n if n.contains("opencode") => (icons::Agent::OpenCode, "bg-raised text-text"),
        n if n.contains("pi") => (icons::Agent::Pi, "bg-raised text-text"),
        _ => (icons::Agent::Generic, "bg-raised text-text"),
    };
    html! {
        span class={ "agent-mark relative grid place-items-center rounded-lg shrink-0 " (size) " " (class) } aria-hidden="true" { (mark.svg()) }
    }
}

/// A one-word state for a chat, with its dot's background class (spelled
/// out whole, so Tailwind finds it).
pub(crate) fn status_word(status: &Status, questions: usize) -> (&'static str, &'static str) {
    if questions > 0 {
        return ("Needs answer", "bg-amber");
    }
    match status {
        Status::Starting => ("Starting", "bg-muted"),
        Status::Idle => ("Idle", "bg-muted"),
        Status::Working => ("Working", "bg-green"),
        Status::Failed { .. } => ("Failed", "bg-red"),
        Status::Exited { .. } => ("Exited", "bg-red"),
    }
}

/// How long ago `then_ms` was, roughly, for a list.
pub(crate) fn elapsed(now_ms: u64, then_ms: u64) -> String {
    display::duration(now_ms.saturating_sub(then_ms))
}

/// The wall clock in milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// A circular gauge of `percent` (0 to 100).
pub(crate) fn ring(percent: u8, label: &str) -> Markup {
    html! {
        span class="ring" style={ "--p:" (percent) } role="img" aria-label=(label) {}
    }
}

/// A folder as a person reads it: the home directory as `~`.
pub(crate) fn display_folder(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_owned(),
    }
}

/// A link the app shell routes without reloading.
pub(crate) fn nav(href: &str, class: &str, body: Markup) -> Markup {
    html! { a href=(href) data-nav class=(class) { (body) } }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
