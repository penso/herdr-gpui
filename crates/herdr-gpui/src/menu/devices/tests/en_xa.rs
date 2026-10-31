//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the device picker
//! and Add Device's SSH form at each step: each draws only translated
//! messages, except for device and provider names and the target typed.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use gpui::{VisualTestContext, point, px, size};

/// Names the rail shows as they are: WSL and the cloud providers.
fn sources() -> Vec<String> {
    let names = ["WSL".to_owned()];
    #[cfg(feature = "cloud")]
    let names = names.into_iter().chain(
        crate::cloud::CloudProvider::ALL
            .iter()
            .map(|&provider| crate::cloud::name(provider).to_owned()),
    );
    names.into_iter().collect()
}

#[test]
fn the_device_picker_shows_only_messages_under_en_xa() {
    under_en_xa("device picker en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1000.), px(800.)));
        draw(cx);
        recorder.take();
        view.update_in(cx, |view, window, cx| {
            if view.open_menu(window, cx) {
                view.menu.anchor = point(px(20.), px(700.));
                view.menu.page = Some(Page::Devices);
                view.menu.selected = Some(0);
            }
        });
        draw(cx);
        let labels: Vec<String> = view.read_with(cx, |view, _| {
            view.endpoints.iter().map(|e| e.label.clone()).collect()
        });
        assert_translated("Device picker", &recorder.take(), &labels);
    });
}

/// Shows the SSH form at `step`.
fn show(view: &gpui::Entity<HerdrWindow>, cx: &mut VisualTestContext, step: Step) {
    view.update(cx, |view, cx| {
        if let Some(setup) = &mut view.menu.device_setup {
            setup.step = step;
        }
        cx.notify();
    });
    draw(cx);
}

#[test]
fn the_ssh_form_shows_only_messages_at_each_step_under_en_xa() {
    under_en_xa("add device en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1000.), px(900.)));
        draw(cx);
        recorder.take();
        view.update_in(cx, |view, window, cx| {
            view.open_add_device(window, cx);
            // What this machine's network and SSH config hold is not drawn.
            if let Some(setup) = &mut view.menu.device_setup {
                setup.discovery = None;
            }
        });
        draw(cx);
        let data: Vec<String> = sources().into_iter().chain(["default".into()]).collect();
        assert_translated("Add Device", &recorder.take(), &data);

        let request = || setup::Request::new("dev@box", "", "").unwrap();
        for (name, step) in [
            ("checking", Step::Checking(request())),
            ("starting", Step::Saving(request(), HostProbe::Stopped)),
            ("saving", Step::Saving(request(), HostProbe::Running)),
            ("fetching the key", Step::FetchingKey(request())),
            ("verifying", Step::Verifying(request(), Offer::Install)),
            ("waiting on macOS", Step::AwaitingLocalNetwork(request())),
            ("install", Step::Confirm(request(), Offer::Install)),
            ("update", Step::Confirm(request(), Offer::Update)),
            ("terminal", Step::Confirm(request(), Offer::Terminal)),
            ("restart", Step::Confirm(request(), Offer::Restart)),
            ("key login", Step::Confirm(request(), Offer::InstallKey)),
            (
                "saved with a warning",
                Step::SavedWarning(setup::SaveOutcome::ExistingServer.warning().unwrap()),
            ),
        ] {
            show(&view, cx, step);
            assert_translated(&format!("Add Device ({name})"), &recorder.take(), &[]);
        }
    });
}

/// Add provider's tiles, and a provider set up in Settings: names and what
/// each provider offers, as herdr-remote words it, are data.
#[cfg(feature = "cloud")]
#[test]
fn the_cloud_panes_show_only_messages_under_en_xa() {
    use crate::cloud::CloudProvider;
    under_en_xa("add provider en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1000.), px(900.)));
        draw(cx);
        recorder.take();
        let data: Vec<String> = sources()
            .into_iter()
            .chain(CloudProvider::ALL.iter().flat_map(|&provider| {
                let offer = crate::cloud::offer(provider);
                [offer.to_owned(), format!("{offer}.")]
            }))
            .chain(["default".into()])
            .collect();
        // Every provider's pane shares one message, so the first one shows
        // what the others would.
        for page in [Page::AddProvider, Page::AddCloud(CloudProvider::ALL[0])] {
            view.update_in(cx, |view, window, cx| {
                view.dismiss_menu(window, cx);
                view.open_add_device(window, cx);
                if let Some(setup) = &mut view.menu.device_setup {
                    setup.discovery = None;
                }
                view.menu.page = Some(page);
            });
            draw(cx);
            assert_translated(&format!("{page:?}"), &recorder.take(), &data);
        }
    });
}

#[test]
fn the_saved_warning_names_a_message() {
    let warning = setup::SaveOutcome::ExistingServer.warning().unwrap();
    assert_eq!(crate::i18n::takes(warning), Some(&[][..]));
}
