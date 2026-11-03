//! Workspace-scoped px0 pages; the workbench owns its checkout processes.
use super::{Editor, GroupId, Location, Store, TabId, WebUrl, view::store};
use crate::i18n::{Localize, fl};
use crate::{
    HerdrWindow,
    px0::{Launcher, State},
    window::Flash,
};
use gpui::{prelude::*, *};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Pages {
    phase: Option<std::mem::Discriminant<State>>,
    /// The server and checkout used to create each page. Reopening a tab or
    /// changing its server rebinds it; stale persisted ports are never loaded.
    bound: HashMap<TabId, (WebUrl, String)>,
}

impl HerdrWindow {
    pub(crate) fn px0_root(&self) -> Option<String> {
        let root = self.local_editor_root()?;
        (root.len() <= 4096 && !root.chars().any(char::is_control)).then(|| root.to_owned())
    }

    pub(crate) fn open_px0(
        &mut self,
        group: Option<GroupId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        if !super::EMBEDDED {
            self.show_flash(Flash::warning(fl!("browser-px0-unsupported")), cx);
            return None;
        }
        if self.px0_root().is_none() {
            self.show_flash(Flash::warning(fl!("browser-px0-no-workspace")), cx);
            return None;
        }
        self.open_embedded_editor(Editor::Px0, group, None, window, cx)
    }

    pub(super) fn ensure_px0_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !super::EMBEDDED {
            return;
        }
        let state = Launcher::state(cx);
        let phase = Some(std::mem::discriminant(&state));
        if self.browser.px0.phase != phase {
            self.browser.px0.phase = phase;
            cx.notify();
        }
        if matches!(state, State::Idle | State::Starting) && !self.browser.px0.bound.is_empty() {
            self.close_editor_pages(Editor::Px0, cx);
            self.browser.px0.bound.clear();
        }
        self.browser
            .px0
            .bound
            .retain(|id, _| store(cx).is_some_and(|store| store.get(*id).is_some()));
        let tab = self
            .browser_key()
            .and_then(|(scope, workspace)| store(cx)?.px0_tab(&scope, &workspace).cloned());
        let Some(mut tab) = tab.filter(|tab| self.group_shows_page(tab.id, cx)) else {
            return;
        };
        let Some(root) = self.px0_root() else {
            return;
        };
        Launcher::want(cx);
        let State::Ready(base) = Launcher::state(cx) else {
            return;
        };
        let binding = (base.clone(), root.clone());
        if self.browser.px0.bound.get(&tab.id) != Some(&binding) {
            let url = match crate::px0::workspace_url(&base, &root) {
                Ok(url) => url,
                Err(error) => {
                    self.browser.failed.insert(tab.id, error.localize().into());
                    return;
                }
            };
            #[cfg(any(target_os = "macos", windows))]
            self.browser.pages.close(tab.id);
            self.browser.failed.remove(&tab.id);
            tab.location = Some(Location::Web { url });
            Store::update(cx, |store| {
                store.visited(tab.id, tab.location.clone(), Some("px0"))
            });
            self.browser.px0.bound.insert(tab.id, binding);
        }
        self.create_editor_page(&tab, window, cx);
    }

    pub(super) fn render_px0_status(
        &self,
        failure: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = if let Some(failure) = failure {
            failure.to_string()
        } else if !super::EMBEDDED {
            fl!("browser-px0-unsupported")
        } else if self.px0_root().is_none() {
            fl!("browser-px0-no-workspace")
        } else {
            match Launcher::state(cx) {
                State::Idle | State::Starting => fl!("browser-px0-starting"),
                State::Ready(_) => fl!("browser-px0-loading"),
                State::Failed(error) => error.localize(),
            }
        };
        super::code_view::centered(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .items_center()
                .debug_selector(|| "px0-placeholder".into())
                .child(text)
                .child(
                    div()
                        .id("px0-retry")
                        .cursor_pointer()
                        .child(fl!("browser-px0-retry"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            Launcher::reset(cx);
                            this.browser.px0.bound.clear();
                            this.ensure_px0_page(window, cx);
                            cx.notify();
                        })),
                ),
        )
    }
}
