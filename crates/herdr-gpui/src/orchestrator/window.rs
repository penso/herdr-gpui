//! The orchestrator in a window of its own. The same view entity a tab
//! hosted moves into it; the main window keeps polling it and feeding it the
//! theme, live agents, and GitHub account from its tick, and acts on its
//! events, until the window closes.

use super::{OrchestratorView, tab::Orchestrator};
use crate::{
    HerdrWindow,
    browser::{Store, TabId},
    window::Flash,
};
use gpui::{prelude::*, *};

/// A window's root: its title bar and the view.
pub(crate) struct OrchestratorWindow {
    view: Entity<OrchestratorView>,
}

impl Render for OrchestratorWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.view.read(cx).theme().clone();
        let header = crate::titlebar::header(&theme, window, |window, _| window.remove_window());
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.background))
            .children(header)
            .child(div().flex_1().min_h_0().flex().child(self.view.clone()))
    }
}

/// An orchestrator moved into its own window.
pub(crate) struct Detached {
    pub(super) orchestrator: Orchestrator,
    pub(super) window: AnyWindowHandle,
}

#[cfg(test)]
impl Detached {
    pub(crate) fn window(&self) -> AnyWindowHandle {
        self.window
    }
}

impl HerdrWindow {
    /// Moves tab `id`'s view into a window of its own and closes the tab.
    pub(crate) fn detach_orchestrator(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(orchestrator) = self.orchestrators.remove(&id) else {
            return;
        };
        let view = orchestrator.view.clone();
        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.), px(400.))),
                titlebar: Some(crate::titlebar::options("Orchestrator")),
                app_owns_titlebar_drag: cfg!(target_os = "macos"),
                ..Default::default()
            },
            |_, cx| cx.new(|_| OrchestratorWindow { view: view.clone() }),
        );
        match opened {
            Ok(handle) => {
                view.update(cx, |view, cx| view.set_detached(true, cx));
                Store::update(cx, |store| store.close(id));
                self.detached_orchestrators.insert(
                    id,
                    Detached {
                        orchestrator,
                        window: handle.into(),
                    },
                );
            }
            Err(error) => {
                tracing::error!(%error, "could not open an Orchestrator window");
                self.orchestrators.insert(id, orchestrator);
                self.show_flash(Flash::warning("Could not open a new window"), cx);
            }
        }
    }

    /// Forgets detached views whose windows closed.
    pub(super) fn forget_closed_orchestrator_windows(&mut self, cx: &App) {
        let open: Vec<WindowId> = cx
            .windows()
            .iter()
            .map(AnyWindowHandle::window_id)
            .collect();
        self.detached_orchestrators
            .retain(|_, detached| open.contains(&detached.window.window_id()));
    }

    /// Tab or window `id`'s view, wherever it is hosted.
    pub(super) fn orchestrator_view(&self, id: TabId) -> Option<&Entity<OrchestratorView>> {
        self.orchestrators
            .get(&id)
            .map(|orchestrator| &orchestrator.view)
            .or_else(|| {
                self.detached_orchestrators
                    .get(&id)
                    .map(|detached| &detached.orchestrator.view)
            })
    }

    /// Every hosted view, in a tab or a window.
    pub(super) fn orchestrator_views(&self) -> Vec<(TabId, Entity<OrchestratorView>)> {
        self.orchestrators
            .iter()
            .map(|(id, orchestrator)| (*id, orchestrator.view.clone()))
            .chain(
                self.detached_orchestrators
                    .iter()
                    .map(|(id, detached)| (*id, detached.orchestrator.view.clone())),
            )
            .collect()
    }
}
