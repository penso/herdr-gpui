//! Explicit rich-text links use the pane's existing document-opening policy.
//! Paths from SSH conversations stay visible and never address this laptop.

use super::*;
use crate::{
    chat::{Target, rich::remote_content},
    terminal::{PaneLink, RowLink, file_target},
};
use herdr_chat_protocol::Cursor;
use std::borrow::Cow;

#[derive(Clone)]
pub(super) struct Links {
    view: WeakEntity<HerdrWindow>,
    target: Option<Target>,
    cursor: Option<Cursor>,
    selection_epoch: u64,
    local: bool,
}

impl Links {
    pub(super) fn new(view: &HerdrWindow, pane_id: &str, cx: &Context<HerdrWindow>) -> Self {
        let pane = view.chats.pane(pane_id);
        Self {
            view: cx.entity().downgrade(),
            target: pane.map(|pane| pane.target.clone()),
            cursor: pane.and_then(|pane| pane.session.since()),
            selection_epoch: view.selection_epoch,
            local: pane.is_some_and(|pane| pane.target.is_local()),
        }
    }

    pub(super) fn content<'a>(&self, content: &'a [Inline]) -> Cow<'a, [Inline]> {
        if self.local {
            Cow::Borrowed(content)
        } else {
            remote_content(content)
        }
    }

    pub(super) fn open(&self, destination: &str, cx: &mut App) {
        if let Ok(url) = crate::browser::WebUrl::try_from(destination) {
            cx.open_url(url.as_str());
            return;
        }
        let Some(target) = self.target.as_ref().filter(|_| self.local) else {
            return;
        };
        let Some(link) = file_target(destination) else {
            return;
        };
        let _ = self.view.update(cx, |view, cx| {
            let current = view
                .chats
                .pane(&target.pane_id)
                .is_some_and(|pane| pane.is_current(target, self.cursor));
            if !current || view.selected_is_remote() || view.selection_epoch != self.selection_epoch
            {
                return;
            }
            let Some(file) = view.file_link(PaneLink {
                pane_id: target.pane_id.clone(),
                link: RowLink {
                    target: link,
                    row: 0,
                    columns: 0..0,
                    continued: Vec::new(),
                },
            }) else {
                return;
            };
            view.open_file_link(
                file,
                view.config.open_files_in == crate::config::FileTarget::Editor,
                cx,
            );
        });
    }
}

#[cfg(test)]
mod tests;
