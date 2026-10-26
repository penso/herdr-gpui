use super::*;

struct Headers {
    view: Entity<HerdrWindow>,
    width: f32,
}

impl Render for Headers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.view.update(cx, |view, cx| {
            let look = super::super::layout::for_config(&view.config, view.fill());
            let endpoint = &view.endpoints[0];
            gpui::div()
                .w(px(self.width))
                .flex()
                .flex_col()
                .text_size(px(view.config.sidebar.size))
                .line_height(px(super::super::line_height(&view.config.sidebar)))
                .child(view.host_row(endpoint, true, look, self.width, false, cx))
                .child(view.host_row(endpoint, true, look, self.width, true, cx))
        })
    }
}

#[gpui::test]
fn cpu_bars_fit_regular_and_pinned_headers_at_every_density(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.show_system_load = true;
            view.system_load.seed(
                crate::usage::Host::Local,
                crate::system_load::Sample {
                    cpu: Some(50.),
                    ..Default::default()
                },
            );
            view
        });
        Headers { view, width: 160. }
    });
    let view = cx.update(|_, cx| fixture.read(cx).view.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    for mode in crate::config::LayoutMode::ALL {
        for width in [160., 232., 480.] {
            for removing in [false, true] {
                fixture.update(cx, |fixture, cx| {
                    fixture.width = width;
                    cx.notify();
                });
                view.update(cx, |view, _| {
                    view.config.layout.mode = mode;
                    if removing {
                        view.menu
                            .removing_devices
                            .insert(crate::endpoint::LOCAL.into());
                    } else {
                        view.menu.removing_devices.clear();
                    }
                });
                cx.update(|window, cx| full_draw(window, cx).clear(cx));
                for (prefix, header, label, bar, status) in [
                    (
                        "",
                        "host-local",
                        "host-label-local",
                        "host-cpu-local",
                        "host-status-local",
                    ),
                    (
                        "sticky-",
                        "sticky-host-local",
                        "sticky-host-label-local",
                        "sticky-host-cpu-local",
                        "sticky-host-status-local",
                    ),
                ] {
                    let header = cx.debug_bounds(header).unwrap();
                    let label = cx.debug_bounds(label).unwrap();
                    let bar = cx.debug_bounds(bar).unwrap();
                    let status = cx.debug_bounds(status).unwrap();
                    let context = format!("{mode}, {width}px, removing={removing}, {prefix}");
                    assert_eq!(bar.size.height, px(3.), "{context}");
                    assert_eq!(bar.left(), label.left(), "{context}");
                    assert!(bar.top() >= label.bottom(), "{context}: {bar:?} {label:?}");
                    assert!(
                        bar.right() <= status.left(),
                        "{context}: {bar:?} {status:?}"
                    );
                    assert!(
                        bar.bottom() <= header.bottom(),
                        "{context}: {bar:?} {header:?}"
                    );
                    assert!(bar.size.width > px(0.), "{context}");
                    assert!(bar.right() <= header.right(), "{context}");
                }
            }
        }
    }
    view.update(cx, |view, _| view.config.show_system_load = false);
    fixture.update(cx, |_, cx| cx.notify());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("host-cpu-local").is_none());
    assert!(cx.debug_bounds("sticky-host-cpu-local").is_none());
}
