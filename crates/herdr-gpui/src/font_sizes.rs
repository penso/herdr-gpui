//! Immediate size previews with one background writer and at most one queued edit per face.
use crate::{
    HerdrWindow,
    config::{Config, FONT_SIZE_RANGE, FontFace},
};
use gpui::{BorrowAppContext, Context, Task};

struct Edit {
    face: FontFace,
    size: f32,
    previous: f32,
}

#[derive(Default)]
pub(crate) struct FontSizeSaves {
    pending: Vec<Edit>,
    task: Option<Task<()>>,
    error: Option<String>,
}

impl FontSizeSaves {
    pub(crate) fn is_busy(&self) -> bool {
        self.task.is_some() || !self.pending.is_empty()
    }

    pub(crate) fn status(&self) -> Option<&str> {
        self.error
            .as_deref()
            .or_else(|| self.is_busy().then_some("Saving font sizes…"))
    }

    fn queue(&mut self, face: FontFace, size: f32, previous: f32) {
        if let Some(edit) = self.pending.iter_mut().find(|edit| edit.face == face) {
            edit.size = size;
        } else {
            self.pending.push(Edit {
                face,
                size,
                previous,
            });
        }
        self.error = None;
    }

    pub(crate) fn apply_pending(&mut self, config: &mut Config) {
        for edit in &mut self.pending {
            // A reload establishes a new rollback baseline, never a new draft.
            edit.previous = edit.face.size(config);
            edit.face.set_size(config, edit.size);
        }
    }

    fn rollback(&mut self, edits: &[Edit], config: &mut Config) {
        for edit in edits {
            if let Some(newer) = self
                .pending
                .iter_mut()
                .find(|newer| newer.face == edit.face)
            {
                newer.previous = edit.previous;
            } else if edit.face.size(config) == edit.size {
                edit.face.set_size(config, edit.previous);
            }
        }
    }
}

impl HerdrWindow {
    pub(crate) fn set_font_size(&mut self, face: FontFace, size: f32, cx: &mut Context<Self>) {
        if !FONT_SIZE_RANGE.contains(&size) || face.size(&self.config) == size {
            return;
        }
        self.font_size_saves
            .queue(face, size, face.size(&self.config));
        face.set_size(&mut self.config, size);
        crate::log_window::set_appearance(&self.config, &self.theme, cx);
        cx.notify();
        self.flush_font_sizes(cx);
    }

    pub(crate) fn flush_font_sizes(&mut self, cx: &mut Context<Self>) {
        self.flush_font_sizes_with(Config::save_font_sizes, cx);
    }

    fn flush_font_sizes_with(
        &mut self,
        save: impl Fn(&[(FontFace, f32)]) -> crate::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.config_load.is_some()
            || self.font_size_saves.task.is_some()
            || self.font_size_saves.pending.is_empty()
        {
            return;
        }
        let edits = std::mem::take(&mut self.font_size_saves.pending);
        let sizes: Vec<_> = edits.iter().map(|edit| (edit.face, edit.size)).collect();
        let saved = cx.background_executor().spawn(async move {
            let result = save(&sizes);
            (result, save)
        });
        self.font_size_saves.task = Some(cx.spawn(async move |this, cx| {
            let (result, save) = saved.await;
            let _ = this.update(cx, |this, cx| {
                this.font_size_saves.task = None;
                match result {
                    Ok(()) => {
                        this.font_size_saves.error = None;
                        for edit in &edits {
                            if edit.face == FontFace::Terminal {
                                this.configured_terminal_size = edit.size;
                            }
                        }
                        if cx.has_global::<crate::app::InitialAppearance>() {
                            cx.update_global::<crate::app::InitialAppearance, _>(
                                |appearance, _| {
                                    for edit in &edits {
                                        edit.face.set_size(&mut appearance.config, edit.size);
                                    }
                                },
                            );
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Could not save font sizes");
                        this.font_size_saves.rollback(&edits, &mut this.config);
                        this.font_size_saves.error =
                            Some(format!("Could not save font sizes: {error}"));
                        crate::log_window::set_appearance(&this.config, &this.theme, cx);
                    }
                }
                // Only completion releases the writer; later clicks replace queued sizes.
                this.flush_font_sizes_with(save, cx);
                cx.notify();
            });
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, point, px};
    use std::sync::{Arc, Mutex};

    #[gpui::test]
    #[allow(clippy::unwrap_used)]
    fn stepper_clicks_preview_and_coalesce_while_the_writer_is_busy(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_preferences(window, cx);
                view.menu
                    .preferences_scroll
                    .set_offset(point(px(0.), px(-120.)));
                view.font_size_saves.task =
                    Some(cx.spawn(async |_, _| std::future::pending().await));
            });
            window.draw(cx).clear(cx);
        });
        for (button, expected) in [
            ("increase", 13.),
            ("increase", 14.),
            ("increase", 15.),
            ("decrease", 14.),
        ] {
            let bounds = cx
                .debug_bounds(if button == "increase" {
                    "preferences-font-sidebar-increase"
                } else {
                    "preferences-font-sidebar-decrease"
                })
                .unwrap();
            cx.simulate_click(bounds.center(), Modifiers::default());
            view.read_with(cx, |view, _| {
                assert_eq!(view.config.sidebar.size, expected);
                assert_eq!(view.font_size_saves.pending.len(), 1);
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        let batches = Arc::new(Mutex::new(Vec::new()));
        let recorded = batches.clone();
        view.update(cx, |view, cx| {
            view.font_size_saves.task = None;
            view.flush_font_sizes_with(
                move |sizes| {
                    recorded.lock().unwrap().push(sizes.to_vec());
                    Ok(())
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(
            *batches.lock().unwrap(),
            vec![vec![(FontFace::Sidebar, 14.)]]
        );
        view.read_with(cx, |view, _| assert!(!view.font_size_saves.is_busy()));
    }

    #[gpui::test]
    #[allow(clippy::unwrap_used)]
    fn completion_writes_the_latest_batch_without_reverting_the_preview(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let batches = Arc::new(Mutex::new(Vec::new()));
        let recorded = batches.clone();
        view.update(cx, |view, cx| {
            view.font_size_saves.queue(FontFace::Sidebar, 13., 12.);
            view.config.sidebar.size = 13.;
            view.flush_font_sizes_with(
                move |sizes| {
                    recorded.lock().unwrap().push(sizes.to_vec());
                    Ok(())
                },
                cx,
            );
            view.set_font_size(FontFace::Sidebar, 14., cx);
            view.set_font_size(FontFace::Sidebar, 15., cx);
            view.set_font_size(FontFace::Terminal, 20., cx);
            assert_eq!(view.config.sidebar.size, 15.);
        });
        cx.run_until_parked();
        assert_eq!(
            *batches.lock().unwrap(),
            vec![
                vec![(FontFace::Sidebar, 13.)],
                vec![(FontFace::Sidebar, 15.), (FontFace::Terminal, 20.)]
            ]
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.config.sidebar.size, 15.);
            assert_eq!(view.config.terminal.size, 20.);
            assert_eq!(view.configured_terminal_size, 20.);
            assert!(!view.font_size_saves.is_busy());
            assert!(view.font_size_saves.error.is_none());
        });
    }

    #[gpui::test]
    fn a_reload_keeps_newer_clicks_without_publishing_them_as_saved(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, cx| {
            view.load_gui_config_with(
                || {
                    let mut config = Config::default();
                    config.sidebar.size = 18.;
                    Ok((config, Default::default()))
                },
                cx,
            );
            view.set_font_size(FontFace::Sidebar, 16., cx);
            // Hold the writer so the load's completion can be observed before saving.
            view.font_size_saves.task = Some(cx.spawn(async |_, _| std::future::pending().await));
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert_eq!(view.config.sidebar.size, 16.);
            assert_eq!(view.font_size_saves.pending[0].previous, 18.);
            assert_eq!(
                cx.global::<crate::app::InitialAppearance>()
                    .config
                    .sidebar
                    .size,
                18.
            );
            view.font_size_saves.task = None;
            view.flush_font_sizes_with(
                |sizes| {
                    assert_eq!(sizes, &[(FontFace::Sidebar, 16.)]);
                    Ok(())
                },
                cx,
            );
        });
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert!(!view.font_size_saves.is_busy());
            assert_eq!(
                cx.global::<crate::app::InitialAppearance>()
                    .config
                    .sidebar
                    .size,
                16.
            );
        });
    }

    #[gpui::test]
    fn save_failure_restores_the_preview_and_reports_the_error(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, cx| {
            view.font_size_saves.queue(FontFace::Sidebar, 13., 12.);
            view.config.sidebar.size = 13.;
            view.flush_font_sizes_with(|_| Err(crate::Error::MissingHome), cx);
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.config.sidebar.size, 12.);
            assert!(!view.font_size_saves.is_busy());
            assert!(
                view.font_size_saves
                    .status()
                    .is_some_and(|message| message.starts_with("Could not save font sizes:"))
            );
        });
    }

    #[test]
    fn repeated_clicks_coalesce_per_face_and_a_reload_preserves_the_latest_draft() {
        let mut saves = FontSizeSaves::default();
        saves.queue(FontFace::Sidebar, 13., 12.);
        saves.queue(FontFace::Sidebar, 14., 13.);
        saves.queue(FontFace::Tabs, 20., 12.);
        assert_eq!(saves.pending.len(), 2);
        let mut config = Config::default();
        config.sidebar.size = 18.;
        saves.apply_pending(&mut config);
        assert_eq!(config.sidebar.size, 14.);
        assert_eq!(config.tabs.size, 20.);
        assert_eq!(saves.pending[0].previous, 18.);
    }

    #[test]
    fn failed_batch_preserves_newer_clicks_and_rolls_back_to_the_last_saved_size() {
        let mut saves = FontSizeSaves::default();
        let mut config = Config::default();
        config.sidebar.size = 14.;
        config.tabs.size = 20.;
        saves.queue(FontFace::Sidebar, 14., 13.);
        saves.rollback(
            &[
                Edit {
                    face: FontFace::Sidebar,
                    size: 13.,
                    previous: 12.,
                },
                Edit {
                    face: FontFace::Tabs,
                    size: 20.,
                    previous: 12.,
                },
            ],
            &mut config,
        );
        assert_eq!(config.sidebar.size, 14.);
        assert_eq!(config.tabs.size, 12.);
        let next = std::mem::take(&mut saves.pending);
        saves.rollback(&next, &mut config);
        assert_eq!(config.sidebar.size, 12.);
    }
}
