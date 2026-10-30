//! probe
use super::*;
use crate::i18n::leak_tests::{draw, under_en_xa};
use crate::sidebar::layout_tests::SidebarFixture;
use gpui::{AppContext as _, px, size};

fn dump(name: &str, lines: Vec<String>) {
    for line in lines {
        if line.chars().filter(|c| c.is_alphabetic()).count() > 1 && !herdr_i18n::pseudo::is_marked(line.trim()) {
            eprintln!("LEAK {name}: {line:?}");
        }
    }
}

#[test]
fn probe_main_window() {
    under_en_xa("main window en-XA leak", |cx, recorder| {
        let (fixture, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut view = fixture_window(window, cx);
                view.live.snapshot = Some(Arc::new(snapshot(6)));
                view.live.status = crate::state::ConnectionStatus::Connected;
                view.endpoints[0].live = view.live.clone();
                view
            });
            SidebarFixture(view)
        });
        let view = cx.update(|_, cx| fixture.read(cx).0.clone());
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, _| window.refresh());
        draw(cx);
        dump("connected", recorder.take());
        view.update(cx, |view, cx| { view.config.layout.mode = crate::config::LayoutMode::Devices; cx.notify(); });
        cx.update(|window, _| window.refresh());
        draw(cx);
        dump("devices", recorder.take());
        view.update(cx, |view, cx| { view.config.layout.mode = Default::default(); view.sidebar_visible = false; cx.notify(); });
        cx.update(|window, _| window.refresh());
        draw(cx);
        dump("rail", recorder.take());
        view.update(cx, |view, cx| { view.sidebar_visible = true; view.live.snapshot = None; view.live.status = crate::state::ConnectionStatus::Disconnected; view.endpoints[0].live = view.live.clone(); cx.notify(); });
        cx.update(|window, _| window.refresh());
        draw(cx);
        dump("disconnected", recorder.take());
    });
}
