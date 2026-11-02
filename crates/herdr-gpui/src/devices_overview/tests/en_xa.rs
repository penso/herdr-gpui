//! The `en-XA` leak test for the Devices overview: its header, totals,
//! activity chart with every device's lane, and the table with a device's
//! agents open.
use super::*;
use crate::i18n::leak_tests::{assert_translated, uncut, under_en_xa};
use gpui::{px, size};

#[test]
fn the_devices_overview_shows_only_messages_and_its_devices() {
    under_en_xa("Devices overview en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        cx.simulate_resize(size(px(2400.), px(1600.)));
        // The window around the tab is other modules' text.
        draw(cx);
        recorder.take();

        open(&view, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                for page in view.devices_overview.pages.values_mut() {
                    page.lanes = true;
                    page.expanded.insert("local".into());
                }
                cx.notify();
            })
        });
        draw(cx);
        // Devices, their agents and checkouts; the tab's title, which the
        // browser names, and agent states, which the sidebar names.
        let data = view.read_with(cx, |view, _| {
            let shared = [
                crate::browser::Location::Devices.default_title(),
                crate::sidebar::status_text(herdr_client::protocol::AgentStatus::Working)
                    .to_owned(),
            ];
            view.overview_devices()
                .into_iter()
                .flat_map(|device| {
                    let agents = device
                        .agents
                        .iter()
                        .flat_map(|agent| [agent.name.clone(), agent.workspace.clone()])
                        .collect::<Vec<_>>();
                    [device.label, device.address].into_iter().chain(agents)
                })
                .chain(shared)
                .collect::<Vec<_>>()
        });
        assert_translated("Devices overview", &uncut(recorder.take()), &data);
    });
}
