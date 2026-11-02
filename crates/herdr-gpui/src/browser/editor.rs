//! Shared shell for HTTP-backed editors. Providers supply launch/readiness and
//! workspace URL policy; identity, groups, persistence and native pages are shared.
use super::{GroupId, Location, Pick, Store, Tab, TabId, view::store};
use crate::i18n::fl;
use crate::{HerdrWindow, window::Flash};
use gpui::{prelude::*, *};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Editor {
    // keep-sorted start
    #[serde(rename = "px0")]
    Px0,
    // Preserve the saved VS Code tab's spelling, including its legacy alias.
    #[serde(rename = "code", alias = "code_group")]
    VsCode,
    // keep-sorted end
}

impl Editor {
    /// The product's name, the same in every language.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Px0 => "px0",
            // i18n: data
            Self::VsCode => "VS Code",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Px0 => "icons/code.svg",
            Self::VsCode => "icons/vscode.svg",
        }
    }
}

impl HerdrWindow {
    /// The starting folder belongs to the selected host, so only a local
    /// workspace may send it to a server on this computer.
    pub(super) fn local_editor_root(&self) -> Option<&str> {
        let endpoint = self.endpoints.get(self.selected_endpoint)?;
        if endpoint.connection.target.is_remote() {
            return None;
        }
        let (_, workspace) = self.browser_key()?;
        crate::palette::launch_root(self.live.snapshot.as_deref()?, &workspace)
            .filter(|root| std::path::Path::new(root).is_absolute())
    }

    pub(super) fn editor_tab_id(&self, editor: Editor, cx: &App) -> Option<TabId> {
        let (scope, workspace) = self.browser_key()?;
        Some(store(cx)?.editor_tab(&scope, &workspace, editor)?.id)
    }

    /// One tab per editor and workspace. Opening it again moves that tab to
    /// the requested group without replacing its page or navigation state.
    pub(super) fn open_embedded_editor(
        &mut self,
        editor: Editor,
        group: Option<GroupId>,
        location: Option<Location>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        if !super::EMBEDDED {
            self.show_flash(Flash::warning(fl!("browser-no-pages")), cx);
            return None;
        }
        let Some((scope, workspace)) = self.browser_key() else {
            self.show_flash(Flash::warning(fl!("browser-open-workspace-first")), cx);
            return None;
        };
        let Some(id) = Store::update(cx, |store| {
            store.open_editor_tab(scope, &workspace, editor, location)
        }) else {
            self.show_flash(Flash::warning(fl!("browser-too-many-tabs")), cx);
            return None;
        };
        match group.or_else(|| self.ensure_layout().map(|layout| layout.active())) {
            Some(group) => self.move_tab_to_group(Pick::Page(id), group, window, cx),
            None => self.show_browser_tab_in(None, id, window, cx),
        }
        Some(id)
    }

    /// Input/tick-time only: providers decide whether their server is ready.
    pub(super) fn ensure_editor_page(
        &mut self,
        editor: Editor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match editor {
            Editor::Px0 => self.ensure_px0_page(window, cx),
            Editor::VsCode => self.ensure_code_page(window, cx),
        }
    }

    pub(super) fn editor_page_settled(&self, id: TabId) -> bool {
        #[cfg(any(target_os = "macos", windows))]
        if self.browser.pages.contains(id) || self.browser.pages.is_building(id) {
            return true;
        }
        self.browser.failed.contains_key(&id)
    }

    /// Only called after the provider's readiness and ownership checks.
    pub(super) fn create_editor_page(
        &mut self,
        tab: &Tab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_page_settled(tab.id) {
            return;
        }
        #[cfg(any(target_os = "macos", windows))]
        if let Err(error) = self.browser.pages.ensure(tab, window, cx) {
            tracing::warn!(%error, "Cannot create an editor page");
            self.browser
                .failed
                .insert(tab.id, crate::i18n::Localize::localize(&error).into());
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = window;
        cx.notify();
    }

    /// Retain tabs and their groups while replacing a provider's pages. Other
    /// editors keep their page state, even when they share a workspace.
    pub(super) fn close_editor_pages(&mut self, editor: Editor, cx: &App) {
        let belongs = |id: TabId| {
            store(cx)
                .and_then(|store| store.get(id))
                .is_some_and(|tab| tab.place.editor() == Some(editor))
        };
        #[cfg(any(target_os = "macos", windows))]
        {
            let ids: Vec<_> = self.browser.pages.ids().filter(|id| belongs(*id)).collect();
            for id in ids {
                self.browser.pages.close(id);
            }
        }
        self.browser.failed.retain(|id, _| !belongs(*id));
    }

    pub(super) fn render_editor_status(
        &self,
        editor: Editor,
        failure: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match editor {
            Editor::Px0 => self.render_px0_status(failure, cx),
            Editor::VsCode => self.render_code_status(failure, cx),
        }
    }
}
