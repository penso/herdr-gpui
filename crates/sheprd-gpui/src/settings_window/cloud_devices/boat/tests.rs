#![allow(clippy::unwrap_used)]
use super::{
    super::super::{Section, SettingsWindow},
    BoatFields, CloudProvider,
};
use crate::config::BoatConfig;
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};

mod counts;
mod credential_ordering;
mod keychain;
mod lifecycle;
mod messages;
mod recovery;
mod setup_script;

fn open(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.boat = BoatConfig::default();
            view.select_section(Section::CloudDevices, window, cx);
            view.cloud_tab = CloudProvider::Boat;
            cx.notify();
        })
    });
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn boat_offers_its_cloud_and_installation_by_default(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(size(px(960.), px(900.)));
    draw(cx);
    assert_eq!(
        cx.debug_bounds("cloud-tab-boat").is_some(),
        CloudProvider::ALL.len() > 1
    );
    assert!(cx.debug_bounds("cloud-boat-card").is_some());
    assert!(cx.debug_bounds("cloud-boat-create").is_some());
    view.read_with(cx, |view, cx| {
        let card = view.boat_card.as_ref().unwrap();
        assert!(card.job.is_none());
        assert!(card.install);
        assert_eq!(
            card.values(cx),
            BoatFields {
                api_url: crate::boat::DEFAULT_API_URL.into(),
                auto_stop: "never".into(),
                ..BoatFields::default()
            }
        );
        assert!(card.key.read(cx).text().is_empty());
    });
}

#[gpui::test]
fn reloaded_config_updates_untouched_fields_and_keeps_edits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let card = view.boat_card.as_mut().unwrap();
            card.fields[2].update(cx, |input, cx| input.set_text_selected("small", cx));
            view.config.boat = BoatConfig {
                api_url: Some("https://boat.example/api/v1".into()),
                machine_type: Some("large".into()),
                environment: Some("base".into()),
                ..BoatConfig::default()
            };
            view.boat_config_changed(cx);
        })
    });
    view.read_with(cx, |view, cx| {
        let values = view.boat_card.as_ref().unwrap().values(cx);
        assert_eq!(values.api_url, "https://boat.example/api/v1");
        assert_eq!(values.machine_type, "small");
        assert_eq!(values.environment, "base");
    });
}
