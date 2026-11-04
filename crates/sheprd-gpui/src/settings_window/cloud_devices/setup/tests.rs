#![allow(clippy::unwrap_used)]
// Not a glob: the parent's `gpui::*` would shadow the `#[test]` that
// `gpui::test` expands to.
use super::super::super::{Section, SettingsWindow};
use crate::{cloud::CloudProvider, cloud_setup::Agent};
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};

fn open(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    // Unconfigured accounts read nothing from disk or the network.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.boat = Default::default();
            view.config.coder = Default::default();
            view.config.daytona = Default::default();
            view.config.cloud = Default::default();
            view.select_section(Section::CloudDevices, window, cx);
        })
    });
    cx.simulate_resize(size(px(960.), px(1600.)));
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// The provider whose card shows a "Run setup script" switch, when one does:
/// Coder's is in Add Device instead.
fn card_provider() -> Option<CloudProvider> {
    CloudProvider::ALL
        .iter()
        .copied()
        .find(|provider| provider.key() != "coder")
}

#[gpui::test]
fn every_agent_has_a_switch_and_cards_offer_setup_only_when_configured(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let provider = card_provider();
    if let Some(provider) = provider {
        view.update(cx, |view, cx| {
            view.cloud_tab = provider;
            cx.notify();
        });
    }
    draw(cx);
    assert!(cx.debug_bounds("cloud-setup").is_some());
    for agent in Agent::ALL {
        let id = format!("cloud-setup-agent-{}", agent.key());
        let id: &'static str = Box::leak(id.into_boxed_str());
        assert!(cx.debug_bounds(id).is_some(), "{id}");
    }
    let Some(provider) = provider else {
        return;
    };
    let switch: &'static str =
        Box::leak(format!("cloud-{}-setup", provider.key()).into_boxed_str());
    assert!(cx.debug_bounds(switch).is_none(), "nothing to run yet");
    view.update(cx, |view, cx| {
        assert_eq!(view.cloud_setup_script(provider, true), None);
        view.config.cloud.agents = Some(vec!["claude".into()]);
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds(switch).is_some());
    view.read_with(cx, |view, _| {
        let script = view.cloud_setup_script(provider, true).unwrap();
        assert!(script.contains("Claude Code"), "{script}");
        // Switched off for the next device, it runs nothing.
        assert_eq!(view.cloud_setup_script(provider, false), None);
    });
}

#[gpui::test]
fn the_card_previews_the_users_own_lines(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    draw(cx);
    assert!(cx.debug_bounds("cloud-setup-preview").is_none());
    view.update(cx, |view, cx| {
        view.config.cloud.setup = Some("echo one\necho two\n".into());
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("cloud-setup-preview").is_some());
}

#[gpui::test]
fn agent_switches_wait_while_settings_load(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.loading = true;
        cx.notify();
    });
    draw(cx);
    let first = Agent::ALL[0];
    let id: &'static str = Box::leak(format!("cloud-setup-agent-{}", first.key()).into_boxed_str());
    let row = cx.debug_bounds(id).unwrap();
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    view.update(cx, |view, cx| {
        assert!(!view.saving, "a click while loading started a save");
        view.toggle_cloud_agent(first, cx);
        assert!(!view.saving, "a toggle while loading started a save");
    });
}

#[test]
fn search_finds_the_setup_card_by_every_agent() {
    use super::super::super::catalog;
    let names = Agent::ALL.iter().map(|agent| agent.label());
    for query in ["setup script"].into_iter().chain(names) {
        let hits = catalog::search(query);
        assert!(
            hits.iter()
                .any(|index| catalog::CATALOG[*index].id == "cloud-setup"),
            "{query}"
        );
    }
}
