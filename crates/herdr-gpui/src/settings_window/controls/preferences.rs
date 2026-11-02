//! Typed preference saves and switches, and the Integrations page.
// i18n: migrated
use super::*;
use crate::config::preferences::Preference;

#[cfg(test)]
#[derive(Clone)]
pub(in crate::settings_window) struct PreferenceIo {
    pub(in crate::settings_window) write:
        std::sync::Arc<dyn Fn(Preference) -> crate::Result<()> + Send + Sync>,
    pub(in crate::settings_window) load:
        std::sync::Arc<dyn Fn() -> crate::Result<super::super::Loaded> + Send + Sync>,
}

#[cfg(test)]
impl PreferenceIo {
    /// Hands every write to `write` and reloads the default configuration.
    pub(in crate::settings_window) fn recording(
        write: impl Fn(Preference) + Send + Sync + 'static,
    ) -> Self {
        Self {
            write: std::sync::Arc::new(move |edit| {
                write(edit);
                Ok(())
            }),
            load: std::sync::Arc::new(crate::settings_window::tests::skill_load),
        }
    }

    /// Hands every write to `write` and reloads through `load`.
    pub(in crate::settings_window) fn with_load(
        write: impl Fn(Preference) + Send + Sync + 'static,
        load: impl Fn() -> crate::Result<super::super::Loaded> + Send + Sync + 'static,
    ) -> Self {
        Self {
            write: std::sync::Arc::new(move |edit| {
                write(edit);
                Ok(())
            }),
            load: std::sync::Arc::new(load),
        }
    }

    /// Every write fails, as when the home directory is missing.
    pub(in crate::settings_window) fn failing() -> Self {
        Self {
            write: std::sync::Arc::new(|_| Err(crate::Error::MissingHome)),
            load: std::sync::Arc::new(crate::settings_window::tests::skill_load),
        }
    }
}

impl SettingsWindow {
    pub(in crate::settings_window) fn save_preference(
        &mut self,
        edit: Preference,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        if let Some(io) = self.controls.preference_io.clone() {
            self.save_with(move || (io.write)(edit), move || (io.load)(), false, cx);
            return;
        }
        self.save_native(move || Config::save_preference(edit), cx);
    }

    pub(in crate::settings_window) fn render_integration_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> Div {
        let query = self.controls.integration_search.read(cx).text().to_owned();
        div()
            .debug_selector(|| "integrations-search".into())
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(self.controls.integration_search.clone())
            .child(
                self.source
                    .update(cx, |source, cx| {
                        source.render_filtered_integrations(&query, cx)
                    })
                    .unwrap_or_else(|_| {
                        div().child(crate::i18n::fl!("settings-integrations-no-window"))
                    }),
            )
    }

    /// A switch that saves `edit` when clicked; saves refuse to overlap, so a
    /// click during a write does nothing.
    pub(in crate::settings_window) fn preference_toggle(
        &self,
        id: impl Into<SharedString>,
        checked: bool,
        edit: Preference,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        super::super::ui::toggle(&self.theme, id, checked, true)
            .on_click(cx.listener(move |this, _, _, cx| this.save_preference(edit.clone(), cx)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::LayoutMode;
    use core::prelude::v1::test;
    use std::sync::{Arc, Mutex};

    #[gpui::test]
    fn switches_route_typed_edits_and_preserve_layout_drafts(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::settings_window::tests::skill_fixture);
        let edits = Arc::new(Mutex::new(Vec::new()));
        let captured = edits.clone();
        view.update(cx, |view, cx| {
            view.controls.preference_io = Some(PreferenceIo::recording(move |edit| {
                captured.lock().unwrap().push(edit)
            }));
            view.accept_layout_choice(LayoutMode::Orca, cx);
        });
        cx.simulate_resize(size(px(960.), px(2200.)));
        for (section, selector, edit) in [
            (
                Section::Tabs,
                "tabs-confirm-tab-toggle",
                Preference::ConfirmCloseTab(false),
            ),
            (
                Section::Tabs,
                "tabs-confirm-pane-toggle",
                Preference::ConfirmClosePane(false),
            ),
            (
                Section::General,
                "general-checkpoints-toggle",
                Preference::AgentCheckpoints(false),
            ),
            (
                Section::General,
                "general-smooth-scroll-toggle",
                Preference::SmoothScroll(false),
            ),
            (
                Section::Tabs,
                "tabs-confirm-workspace-toggle",
                Preference::ConfirmCloseWorkspace(false),
            ),
            (
                Section::General,
                "general-merged-worktrees-toggle",
                Preference::RemoveMergedWorktrees(true),
            ),
            (
                Section::Notifications,
                "notif-finished-toggle",
                Preference::NotificationFinished(false),
            ),
        ] {
            view.update(cx, |view, cx| {
                view.section = section;
                cx.notify();
            });
            cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
            let point = cx.debug_bounds(selector).unwrap().center();
            cx.simulate_click(point, Default::default());
            cx.run_until_parked();
            assert_eq!(edits.lock().unwrap().last(), Some(&edit));
            view.read_with(cx, |view, _| {
                assert!(!view.busy());
                assert_eq!(view.config.layout.mode, LayoutMode::Orca);
                assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
            });
            let count = edits.lock().unwrap().len();
            view.update(cx, |view, cx| {
                view.saving = true;
                cx.notify();
            });
            cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
            let point = cx.debug_bounds(selector).unwrap().center();
            cx.simulate_click(point, Default::default());
            cx.run_until_parked();
            assert_eq!(edits.lock().unwrap().len(), count);
            view.update(cx, |view, _| view.saving = false);
        }
    }
}
