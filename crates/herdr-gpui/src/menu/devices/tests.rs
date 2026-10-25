use super::{
    Page,
    add_device::{Offer, Setup, Step},
    setup,
};
use crate::{HerdrWindow, search_input::SearchInput, sidebar::layout_tests::fixture_window};
use gpui::{AppContext, Context};
use herdr_client::HostProbe;

#[cfg(feature = "cloud")]
mod add_provider;
#[cfg(feature = "boat")]
mod boat_rail;
#[cfg(feature = "cloud")]
mod cloud_providers;
mod host_key;
mod key_login;
mod local_network;
#[cfg(feature = "cloud")]
mod rail;
#[cfg(unix)]
mod setup_activation;
#[cfg(unix)]
mod setup_workspace;
mod version_warning;

#[test]
fn only_a_present_herdr_is_saved_without_a_terminal() {
    assert_eq!(Offer::for_probe(HostProbe::Running), None);
    assert_eq!(Offer::for_probe(HostProbe::Stopped), None);
    assert_eq!(Offer::for_probe(HostProbe::Missing), Some(Offer::Install));
    assert_eq!(Offer::for_probe(HostProbe::Outdated), Some(Offer::Update));
    assert_eq!(
        Offer::for_probe(HostProbe::SshFailed(herdr_client::SshFailure::HostKey)),
        Some(Offer::Terminal)
    );
    assert_eq!(
        Offer::for_probe(HostProbe::SshFailed(herdr_client::SshFailure::Auth)),
        Some(Offer::InstallKey)
    );
    let question = Offer::Install.question("dev@box", Some("dev"));
    if cfg!(windows) {
        // No terminal setup: the user is told what to run instead.
        assert!(question.contains("Herdr was not detected there"));
        assert!(question.contains("`herdr machine add dev@box`"));
    } else {
        assert_eq!(
            question,
            "Herdr was not detected on dev@box. Should we install it?"
        );
    }
}

fn open_form(view: &mut HerdrWindow, step: Step, cx: &mut Context<HerdrWindow>) {
    view.menu.device_setup = Some(Setup {
        fields: std::array::from_fn(|_| cx.new(SearchInput::new)),
        step,
        claim: None,
        task: None,
        discovery: None,
        suggested_label: None,
        _edits: Vec::new(),
        highlight: None,
        suggestion_scroll: gpui::ScrollHandle::new(),
        error: None,
    });
    view.menu.page = Some(Page::AddDevice);
}

fn step(view: &HerdrWindow) -> &Step {
    &view.menu.device_setup.as_ref().unwrap().step
}

#[gpui::test]
fn a_saved_device_closes_the_dialog_by_itself(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            open_form(view, Step::Saved, cx);
            view.poll_device_setup(window, cx);
            assert!(view.menu.page.is_none());
            assert!(view.menu.device_setup.is_none());
            assert!(view.focus.is_focused(window));
        });
    });
}

/// The sessions list can attach a saved device to another of its sessions.
/// The catalog still holds the entry it was saved with, so adding that entry
/// again is refused, and the session it now shows was never saved.
#[gpui::test]
fn a_device_on_another_session_is_still_matched_by_its_saved_entry(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.reconcile_catalog(
                vec![herdr_client::SavedHost {
                    id: "0123456789abcdef0123456789abcdef".into(),
                    label: "m5max-ms".into(),
                    target: "penso@box".into(),
                    session: "default".into(),
                    enabled: true,
                }],
                Vec::new(),
                cx,
            );
            // What choosing another session from the list does to the target.
            view.endpoints[1].connection.target = herdr_client::ConnectTarget::Ssh {
                target: "penso@box".into(),
                session: "work".into(),
            };
            let saved = setup::Request::new("penso@box", "Again", "").unwrap();
            assert!(matches!(
                view.ensure_new_device(&saved),
                Err(crate::Error::Devices(super::Error::Exists(label))) if label == "m5max-ms"
            ));
            let work = setup::Request::new("penso@box", "Work", "work").unwrap();
            assert!(view.ensure_new_device(&work).is_ok());
        });
    });
}

#[gpui::test]
fn a_host_already_saved_or_being_added_returns_to_the_form(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(crate::endpoint::Endpoint::new(
                "0123456789abcdef0123456789abcdef".into(),
                "m5max-ms".into(),
                herdr_client::ConnectTarget::Ssh {
                    target: "penso@box".into(),
                    session: "default".into(),
                },
                true,
            ));
            // The window's own list refuses the exact spelling at once.
            let again = setup::Request::new("penso@box", "Again", "").unwrap();
            assert!(matches!(
                view.ensure_new_device(&again),
                Err(crate::Error::Devices(super::Error::Exists(label))) if label == "m5max-ms"
            ));
            let other = setup::Request::new("penso@box", "Work", "work").unwrap();
            assert!(view.ensure_new_device(&other).is_ok());

            // Background checks catch other spellings and concurrent adds.
            // Neither may fall through to offering a terminal.
            for (error, message) in [
                (
                    crate::Error::from(super::Error::Exists("m5max-ms".into())),
                    "This host and session are already saved as \u{201c}m5max-ms\u{201d}.",
                ),
                (
                    crate::Error::from(super::Error::Adding),
                    "This host is already being added, in another Add Device dialog or a \u{201c}Set up\u{201d} workspace that is still running. Finish or close it, then try again.",
                ),
            ] {
                open_form(view, Step::Checking(again.clone()), cx);
                view.device_probed(Err(error), cx);
                assert!(matches!(step(view), Step::Form));
                assert_eq!(
                    view.menu.device_setup.as_ref().unwrap().error.as_deref(),
                    Some(message)
                );
            }
            open_form(view, Step::Verifying(again.clone(), Offer::Install), cx);
            view.setup_space_verified(Err(super::Error::Exists("m5max-ms".into()).into()), cx);
            assert!(matches!(step(view), Step::Form));
        });
    });
}
