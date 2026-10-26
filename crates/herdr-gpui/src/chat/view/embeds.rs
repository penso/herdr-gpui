//! What an agent message shows in place of things the app loads only when
//! asked: a page from an HTML fence, live in the message once shown, a
//! picture on the web, and the caption of a picture on the agent's machine
//! that follows the message.

use super::{
    super::{
        page::{PAGE_HEIGHT, PageKey, PageSource},
        visual::{Source, VisualId, Visuals},
    },
    visuals::Frame,
};
use crate::{HerdrWindow, browser::WebUrl, config::corners, notifications::safe_text};
use gpui::{prelude::*, *};
use herdr_chat_protocol::EntryId;
use std::{path::PathBuf, rc::Rc};

/// What the button says where it shows the page.
const SHOW: &str = if crate::browser::EMBEDDED {
    "Show page"
} else {
    "Open in browser"
};

impl HerdrWindow {
    /// An HTML fence: its page live in the message once shown, until then
    /// a card that says what it is and shows it.
    pub(super) fn render_chat_html(
        &self,
        pane_id: &str,
        entry: EntryId,
        part: usize,
        page: &Rc<str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = PageKey { entry, part };
        if let Some(live) = self.render_chat_live_page(pane_id, key, cx) {
            return live.into_any_element();
        }
        let lines = page.lines().count();
        let source = PageSource::Html(page.clone());
        self.embed_card("icons/globe.svg")
            .debug_selector(move || format!("chat-html-{part}"))
            .child(div().flex_1().min_w_0().child(match lines {
                1 => "HTML page \u{b7} 1 line".to_owned(),
                count => format!("HTML page \u{b7} {count} lines"),
            }))
            .child(self.show_button(pane_id, ("chat-show-page", part), key, source, cx))
            .into_any_element()
    }

    /// Under a tool call that wrote an HTML file: the page live once shown,
    /// until then a button that shows it.
    pub(super) fn render_chat_file_page(
        &self,
        pane_id: &str,
        entry: EntryId,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = PageKey {
            entry,
            part: PageKey::TOOL,
        };
        if let Some(live) = self.render_chat_live_page(pane_id, key, cx) {
            return live.into_any_element();
        }
        div()
            .flex()
            .child(self.show_button(
                pane_id,
                ("chat-show-file", entry.0),
                key,
                PageSource::File(path),
                cx,
            ))
            .into_any_element()
    }

    fn show_button(
        &self,
        pane_id: &str,
        id: impl Into<ElementId>,
        key: PageKey,
        source: PageSource,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let pane_id = pane_id.to_owned();
        let fill = theme.primary();
        div()
            .id(id)
            .debug_selector(|| "chat-show-page".into())
            .flex_none()
            .px(px(10.))
            .h(px(24.))
            .flex()
            .items_center()
            .rounded(px(corners::CONTROL))
            .bg(rgb(fill))
            .text_color(rgb(theme.text_on(fill)))
            .text_size(self.chat_small_text())
            .cursor_pointer()
            .child(SHOW)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.show_chat_page(&pane_id, key, source.clone(), window, cx);
            }))
    }

    /// The page at `key`, live in the message under a bar that hides it.
    /// Its body paints nothing: on macOS the page shows from beneath the
    /// window's drawing there.
    fn render_chat_live_page(
        &self,
        pane_id: &str,
        key: PageKey,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let page = self
            .chats
            .pane(pane_id)?
            .pages
            .iter()
            .find(|page| page.key == key)?;
        let theme = &self.theme;
        let pane = pane_id.to_owned();
        let hide = div()
            .id(("chat-hide-page", page.id_number()))
            .debug_selector(|| "chat-hide-page".into())
            .flex_none()
            .px(px(8.))
            .h(px(22.))
            .flex()
            .items_center()
            .gap(px(4.))
            .rounded(px(corners::CONTROL))
            .text_size(self.chat_small_text())
            .text_color(rgb(theme.subtext()))
            .cursor_pointer()
            .hover(|button| button.bg(rgb(theme.active)))
            .child(
                svg()
                    .path("icons/close.svg")
                    .size(px(9.))
                    .text_color(rgb(theme.subtext())),
            )
            .child("Hide")
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.hide_chat_page(&pane, key, cx);
            }));
        #[cfg(any(target_os = "macos", windows))]
        let body = match self.chat_page_element(page.id) {
            Some(page) => div().size_full().child(page),
            None => opening(theme),
        };
        #[cfg(not(any(target_os = "macos", windows)))]
        let body = opening(theme);
        Some(
            div()
                .debug_selector(|| "chat-page".into())
                .w_full()
                .flex()
                .flex_col()
                .rounded(px(corners::CONTROL))
                .border_1()
                .border_color(rgb(theme.active))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(8.))
                        .h(px(30.))
                        .border_b_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.surface))
                        .child(
                            svg()
                                .flex_none()
                                .path("icons/globe.svg")
                                .size(px(12.))
                                .text_color(rgb(theme.subtext())),
                        )
                        .child(div().flex_1().min_w_0().truncate().child("HTML page"))
                        .child(hide),
                )
                .child(div().w_full().h(px(PAGE_HEIGHT)).relative().child(body)),
        )
    }

    /// A picture on the web: drawn once loaded, until then a card that
    /// names its host and loads it on request.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_chat_remote_picture(
        &self,
        pane_id: &str,
        entry: EntryId,
        index: usize,
        id: VisualId,
        url: &WebUrl,
        alt: &str,
        visuals: &Visuals,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if visuals.wants(id) {
            return self
                .render_chat_visual(id, visuals, Frame::Image)
                .into_any_element();
        }
        let theme = &self.theme;
        let pane_id = pane_id.to_owned();
        let url = url.clone();
        let host = safe_text(url.host(), 120);
        self.embed_card("icons/zoom.svg")
            .debug_selector(move || format!("chat-remote-{index}"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().truncate().child(if alt.is_empty() {
                        "A picture on the web".to_owned()
                    } else {
                        alt.to_owned()
                    }))
                    .child(
                        div()
                            .text_size(px((self.chat_text_size() - 2.).max(10.)))
                            .text_color(rgb(theme.muted))
                            .truncate()
                            .child(format!("Loading it tells {host} this machine's address")),
                    ),
            )
            .child(
                div()
                    .id(("chat-load-picture", index))
                    .debug_selector(|| "chat-load-picture".into())
                    .flex_none()
                    .px(px(10.))
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .rounded(px(corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .text_size(self.chat_small_text())
                    .cursor_pointer()
                    .hover(|button| button.bg(rgb(theme.active)))
                    .child(format!("Load from {host}"))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        if let Some(pane) = this.chats.pane_mut(&pane_id) {
                            pane.prepared.visuals.want(id, Source::Remote(url.clone()));
                            pane.remeasure(entry);
                        }
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// The caption of a picture on the agent's machine; herdr-acp shows the
    /// picture itself after the message.
    pub(super) fn render_chat_picture_caption(&self, alt: &str) -> AnyElement {
        let theme = &self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(self.chat_small_text())
            .text_color(rgb(theme.muted))
            .child(
                svg()
                    .path("icons/zoom.svg")
                    .size(px(11.))
                    .text_color(rgb(theme.muted)),
            )
            .child(if alt.is_empty() {
                "Picture".to_owned()
            } else {
                alt.to_owned()
            })
            .into_any_element()
    }

    fn embed_card(&self, icon: &'static str) -> Div {
        let theme = &self.theme;
        div()
            .max_w(px(520.))
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(10.))
            .py(px(8.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.surface))
            .child(
                svg()
                    .flex_none()
                    .path(icon)
                    .size(px(14.))
                    .text_color(rgb(theme.subtext())),
            )
    }
}

fn opening(theme: &crate::config::Theme) -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(theme.background))
        .text_color(rgb(theme.muted))
        .child("Opening the page\u{2026}")
}
