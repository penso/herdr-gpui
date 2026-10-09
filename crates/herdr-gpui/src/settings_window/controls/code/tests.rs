#![allow(clippy::unwrap_used)]
use super::*;
use crate::settings_window::controls::tests::{skill_fixture, skill_load};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext, px, size};
use std::sync::{Arc, Mutex};

/// The addresses the page saved, in order; `None` removed it.
type Saves = Arc<Mutex<Vec<Option<WebUrl>>>>;

const COMMIT: &str = "2a59476c9bfcb90b3ddc372c36762471b7dfad1c";

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// The VS Code page, with its saves recorded rather than written.
fn page(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext, Saves) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let saves = Arc::new(Mutex::new(Vec::new()));
    let recorded = saves.clone();
    cx.simulate_resize(size(px(960.), px(1200.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.code.io = Some(CodeIo {
                write: Arc::new(move |url| {
                    recorded.lock().unwrap().push(url);
                    Ok(())
                }),
                load: skill_load,
            });
            view.select_section(Section::Code, window, cx);
        })
    });
    draw(cx);
    (view, cx, saves)
}

fn type_address(view: &Entity<SettingsWindow>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        let focus = view
            .read(cx)
            .code
            .field
            .as_ref()
            .unwrap()
            .input
            .read(cx)
            .focus
            .clone();
        window.focus(&focus, cx);
    });
    cx.simulate_input(text);
}

#[gpui::test]
fn the_page_saves_an_address_on_enter_and_escape_puts_it_back(cx: &mut TestAppContext) {
    let (view, cx, saves) = page(cx);
    assert!(cx.debug_bounds("settings-code-url").is_some());
    assert!(cx.debug_bounds("settings-code-test").is_some());

    type_address(&view, cx, "localhost:8000/?tkn=x");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        saves.lock().unwrap().as_slice(),
        [Some(
            WebUrl::try_from("http://localhost:8000/?tkn=x").unwrap()
        )]
    );

    // Escape drops an edit; nothing more is saved.
    type_address(&view, cx, "127.0.0.1:9000");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(saves.lock().unwrap().len(), 1);
    view.read_with(cx, |view, cx| {
        let field = view.code.field.as_ref().unwrap();
        assert_eq!(field.input.read(cx).text(), view.saved_code_url());
    });
}

#[gpui::test]
fn what_is_not_an_address_is_kept_unsaved_and_flagged(cx: &mut TestAppContext) {
    let (view, cx, saves) = page(cx);
    type_address(&view, cx, "file:///etc/passwd");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    draw(cx);
    assert!(saves.lock().unwrap().is_empty());
    view.read_with(cx, |view, cx| {
        let field = view.code.field.as_ref().unwrap();
        assert!(field.invalid);
        assert_eq!(field.input.read(cx).text(), "file:///etc/passwd");
    });
}

fn answers(_: &WebUrl) -> crate::Result<Server> {
    Ok(Server::from_version(COMMIT).unwrap())
}

fn unreachable(_: &WebUrl) -> crate::Result<Server> {
    Err(crate::Error::CodeNotServer { status: 404 })
}

#[gpui::test]
fn testing_the_connection_shows_what_the_server_said(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    view.update(cx, |view, cx| {
        view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/?tkn=x").unwrap());
        cx.notify();
    });
    for (probe, expected) in [
        (
            answers as fn(&WebUrl) -> crate::Result<Server>,
            Test::Passed(Server::from_version(COMMIT).unwrap()),
        ),
        (
            unreachable,
            Test::Failed("This address is not a VS Code server (HTTP 404).".into()),
        ),
    ] {
        view.update(cx, |view, _| view.code.probe = probe);
        draw(cx);
        let button = cx.debug_bounds("settings-code-test").unwrap();
        cx.simulate_click(button.center(), Default::default());
        cx.run_until_parked();
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.code.test, expected));
        assert!(cx.debug_bounds("settings-code-result").is_some());
    }
}

/// A reload, after a save or a change to the file, keeps an edit in
/// progress, and the field follows the file again once nothing is edited.
#[gpui::test]
fn a_reload_keeps_an_address_edit_in_progress(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    let reload = |view: &Entity<SettingsWindow>, cx: &mut VisualTestContext, address: &str| {
        view.update(cx, |view, cx| {
            view.config.code.url = Some(WebUrl::try_from(address).unwrap());
            view.sync_code_field(cx);
        });
    };
    let text = |view: &Entity<SettingsWindow>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.code
                .field
                .as_ref()
                .unwrap()
                .input
                .read(cx)
                .text()
                .to_owned()
        })
    };

    type_address(&view, cx, "127.0.0.1:9000/?tkn=typing");
    reload(&view, cx, "http://127.0.0.1:8000/?tkn=file");
    assert_eq!(text(&view, cx), "127.0.0.1:9000/?tkn=typing");

    // Escape gives the edit up, and the field shows what the file holds.
    cx.simulate_keystrokes("escape");
    assert_eq!(text(&view, cx), "http://127.0.0.1:8000/?tkn=file");
    reload(&view, cx, "http://127.0.0.1:8000/?tkn=changed");
    assert_eq!(text(&view, cx), "http://127.0.0.1:8000/?tkn=changed");
}

/// An address submitted while another save runs waits for it, rather than
/// being dropped and then replaced by the older saved address.
#[gpui::test]
fn an_address_submitted_during_a_save_is_saved_after_it(cx: &mut TestAppContext) {
    let (view, cx, saves) = page(cx);
    view.update(cx, |view, _| view.saving = true);
    type_address(&view, cx, "127.0.0.1:9000/?tkn=later");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(saves.lock().unwrap().is_empty());

    // The earlier save ends: its reload keeps the field, then the address
    // waiting is saved.
    view.update(cx, |view, cx| {
        view.saving = false;
        view.sync_code_field(cx);
        let field = view.code.field.as_ref().unwrap();
        assert_eq!(
            field.input.read(cx).text(),
            "http://127.0.0.1:9000/?tkn=later"
        );
        view.sync_controls(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        saves.lock().unwrap().as_slice(),
        [Some(
            WebUrl::try_from("http://127.0.0.1:9000/?tkn=later").unwrap()
        )]
    );
}

/// An address whose save failed stays in the field as an edit, rather
/// than being replaced by the older saved one, and Enter tries it again.
#[gpui::test]
fn an_address_whose_save_failed_stays_in_the_field(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    let tries = Arc::new(Mutex::new(0));
    let counted = tries.clone();
    view.update(cx, |view, _| {
        view.code.io = Some(CodeIo {
            write: Arc::new(move |_| {
                *counted.lock().unwrap() += 1;
                Err(crate::Error::CodeTokenRefused)
            }),
            load: skill_load,
        });
    });
    type_address(&view, cx, "127.0.0.1:9000/?tkn=x");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let text = |view: &Entity<SettingsWindow>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let field = view.code.field.as_ref().unwrap();
            field.input.read(cx).text().to_owned()
        })
    };
    assert_eq!(*tries.lock().unwrap(), 1, "not retried on its own");
    assert_eq!(text(&view, cx), "http://127.0.0.1:9000/?tkn=x");

    // A later reload keeps it too, and Enter saves it again.
    view.update(cx, |view, cx| view.sync_code_field(cx));
    assert_eq!(text(&view, cx), "http://127.0.0.1:9000/?tkn=x");
    type_address(&view, cx, "");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(*tries.lock().unwrap(), 2);
}

/// Showing the page again while an address save runs leaves the field and
/// the save alone; the reload that ends a successful save then has the
/// field follow the file again.
#[gpui::test]
fn showing_the_page_during_a_save_leaves_it_to_finish(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    let address = "http://127.0.0.1:9000/?tkn=x";
    let url = WebUrl::try_from(address).unwrap();
    let text = |view: &Entity<SettingsWindow>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let field = view.code.field.as_ref().unwrap();
            field.input.read(cx).text().to_owned()
        })
    };
    // The save of `address` is on its way.
    view.update(cx, |view, cx| {
        view.code.field.as_mut().unwrap().show(address, cx);
        view.code.saving = Some(Some(url.clone()));
        view.saving = true;
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_code_page(window, cx)));
    assert_eq!(text(&view, cx), address);
    view.read_with(cx, |view, _| assert!(view.code.saving.is_some()));

    // It succeeds, and later changes to the file show in the field.
    view.update(cx, |view, cx| {
        view.saving = false;
        view.config.code.url = Some(url);
        view.sync_code_field(cx);
        view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:9000/?tkn=new").unwrap());
        view.sync_code_field(cx);
    });
    assert_eq!(text(&view, cx), "http://127.0.0.1:9000/?tkn=new");
}

/// A reload that shows another address forgets the last test's result, and
/// an answer still on its way, which were of the address shown before.
#[gpui::test]
fn a_reload_to_another_address_forgets_the_last_test(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    view.update(cx, |view, cx| {
        view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/?tkn=x").unwrap());
        view.sync_code_field(cx);
        view.code.test = Test::Passed(Server::from_version(COMMIT).unwrap());
        let before = view.code.generation;
        view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:9000/?tkn=y").unwrap());
        view.sync_code_field(cx);
        assert_eq!(view.code.test, Test::Idle);
        assert_ne!(view.code.generation, before);
    });
}

/// Quitting while an address waits for another save still saves it, after
/// that save.
#[gpui::test]
fn quitting_saves_an_address_waiting_for_another_save(cx: &mut TestAppContext) {
    let (view, cx, saves) = page(cx);
    view.update(cx, |view, _| view.saving = true);
    type_address(&view, cx, "127.0.0.1:9000/?tkn=later");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(saves.lock().unwrap().is_empty());

    let quit = view.update(cx, |view, cx| {
        view.shutdown_with(|_| panic!("no font edits"), cx)
    });
    cx.run_until_parked();
    let (done, result) = std::sync::mpsc::sync_channel(1);
    cx.executor()
        .spawn(async move {
            done.send(quit.await).unwrap();
        })
        .detach();
    cx.run_until_parked();
    result.recv().unwrap().unwrap();
    assert_eq!(
        saves.lock().unwrap().as_slice(),
        [Some(
            WebUrl::try_from("http://127.0.0.1:9000/?tkn=later").unwrap()
        )]
    );
}
