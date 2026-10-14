#![allow(clippy::unwrap_used)]

use super::*;
use crate::settings_window::controls::{preferences::PreferenceIo, tests::skill_load};
use gpui::VisualTestContext;
use std::sync::{Arc, mpsc};

mod busy;
mod detected;
mod disconnected;
mod installed;
mod saved;

fn fixture(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    cx.add_window_view(|_, cx| {
        let mut view = SettingsWindow::new(weak, cx);
        view.section = Section::StatusBar;
        view.config = Config::default();
        view
    })
}

fn cached(ids: &[&str]) -> Vec<usage::Reading> {
    usage::providers()
        .filter(|provider| ids.contains(&provider.id()))
        .map(|provider| usage::Reading {
            provider,
            report: None,
            error: None,
            access: Some(usage::Access::Needed),
        })
        .collect()
}

fn install(
    cx: &mut VisualTestContext,
    view: &Entity<SettingsWindow>,
    readings: Vec<usage::Reading>,
) {
    let source = view.read_with(cx, |view, _| view.source.upgrade().unwrap());
    source.update(cx, |source, cx| {
        source
            .usage
            .set_current_for_test(usage::Host::Local, readings);
        cx.notify();
    });
    cx.run_until_parked();
}

fn recording(
    cx: &mut VisualTestContext,
    view: &Entity<SettingsWindow>,
) -> mpsc::Receiver<Preference> {
    let (captured, edits) = mpsc::channel();
    view.update(cx, |view, _| {
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                captured.send(edit).unwrap();
                Ok(())
            }),
            load: skill_load,
        });
    });
    cx.simulate_resize(size(px(960.), px(4000.)));
    edits
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn click(
    cx: &mut VisualTestContext,
    edits: &mpsc::Receiver<Preference>,
    selector: &'static str,
) -> Option<Preference> {
    draw(cx);
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Default::default());
    cx.run_until_parked();
    edits.try_recv().ok()
}
