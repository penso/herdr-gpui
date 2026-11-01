//! Settings > Machine sync: turning it on asks first, and only for an
//! address and name the other machines could save; each saved device has a
//! checkbox.
use super::*;
use crate::machine_sync::MachineSync;
use gpui::{px, size};
use herdr_client::{SavedHost, machine_sync::SyncState};

fn host(n: u8) -> SavedHost {
    SavedHost {
        id: format!("{n:032x}"),
        label: format!("box-{n}"),
        target: format!("10.0.0.{n}"),
        session: "default".to_owned(),
        enabled: true,
    }
}

fn page(
    sync: MachineSync,
    cx: &mut TestAppContext,
) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    cx.update(|cx| cx.set_global(sync));
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(2000.)));
    view.update(cx, |view, cx| {
        view.section = Section::MachineSync;
        view.make_machine_sync_fields(cx);
    });
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn type_address(view: &Entity<SettingsWindow>, text: &str, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        let input = view.machines.address.clone().unwrap();
        input.update(cx, |input, cx| input.set_text_at(text, text.len(), cx));
    });
}

#[gpui::test]
fn turning_on_asks_before_anything_changes(cx: &mut TestAppContext) {
    let (view, cx) = page(MachineSync::fixture(None, vec![host(2), host(3)]), cx);
    draw(cx);
    assert!(cx.debug_bounds("machine-sync-confirm").is_none());
    assert!(cx.debug_bounds("machine-sync-host-0").is_some());
    assert!(cx.debug_bounds("machine-sync-host-1").is_some());
    type_address(&view, "me@laptop", cx);
    view.update(cx, |view, cx| view.ask_machine_sync(cx));
    draw(cx);
    assert!(cx.debug_bounds("machine-sync-confirm").is_some());
    // Asking saved nothing: sync is still off.
    cx.update(|_, cx| assert!(cx.global::<MachineSync>().state().is_none()));
}

#[gpui::test]
fn an_address_others_could_not_save_is_refused_before_asking(cx: &mut TestAppContext) {
    let (view, cx) = page(MachineSync::fixture(None, vec![host(2)]), cx);
    type_address(&view, "-oProxyCommand=x", cx);
    view.update(cx, |view, cx| view.ask_machine_sync(cx));
    view.read_with(cx, |view, _| {
        assert!(!view.machines.confirming);
        assert!(view.machines.invalid.is_some());
    });
    draw(cx);
    assert!(cx.debug_bounds("machine-sync-confirm").is_none());
}

#[gpui::test]
fn while_on_each_device_shows_its_checkbox_and_sync_now(cx: &mut TestAppContext) {
    let mut state =
        SyncState::new(format!("{:032x}", 1), "laptop".into(), "me@laptop".into()).unwrap();
    state.excluded.insert(host(3).id);
    let (view, cx) = page(
        MachineSync::fixture(Some(state), vec![host(2), host(3)]),
        cx,
    );
    draw(cx);
    assert!(cx.debug_bounds("machine-sync-now").is_some());
    assert!(cx.debug_bounds("machine-sync-save").is_some());
    assert!(cx.debug_bounds("machine-sync-host-1").is_some());
    // The fields show the saved address, not a guess.
    view.read_with(cx, |view, cx| {
        let address = view
            .machines
            .address
            .as_ref()
            .unwrap()
            .read(cx)
            .text()
            .to_owned();
        assert_eq!(address, "me@laptop");
    });
}

fn field(view: &Entity<SettingsWindow>, name: bool, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, cx| {
        let input = if name {
            &view.machines.name
        } else {
            &view.machines.address
        };
        input.as_ref().unwrap().read(cx).text().to_owned()
    })
}

#[gpui::test]
fn the_saved_state_replaces_a_guess_shown_before_it_loaded(cx: &mut TestAppContext) {
    // The page opened before the read finished, so it shows a guess.
    let (view, cx) = page(MachineSync::fixture(None, vec![host(2)]), cx);
    type_address(&view, "edited@laptop", cx);
    let state = SyncState::new(format!("{:032x}", 1), "laptop".into(), "me@laptop".into()).unwrap();
    cx.update(|_, cx| cx.set_global(MachineSync::fixture(Some(state), vec![host(2)])));
    view.update(cx, |view, cx| view.update_machine_sync_suggestion(cx));
    // The untouched name takes the saved one; the edited address is kept.
    assert_eq!(field(&view, true, cx), "laptop");
    assert_eq!(field(&view, false, cx), "edited@laptop");

    // Reopening the page drops the edit and shows what is saved.
    view.update(cx, |view, cx| {
        view.close_machine_sync();
        view.make_machine_sync_fields(cx);
    });
    assert_eq!(field(&view, false, cx), "me@laptop");
}
