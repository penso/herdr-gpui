use super::*;
#[cfg(any(target_os = "macos", windows))]
use crate::browser::code::Reach;
use crate::{code_server::Server, controls::Command};
use gpui::{Modifiers, MouseButton, point, px};

fn run(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, command: Command) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
    draw(cx);
}

fn focus_workspace(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, id: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown.focused_workspace_id = Some(id.into());
            view.live.snapshot = Some(Arc::new(shown));
        });
    });
    draw(cx);
}

#[gpui::test]
fn each_workspace_shows_its_own_panel_beside_its_groups(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    assert!(cx.debug_bounds("code").is_none());

    run(&view, cx, Command::ToggleCode);
    let panel = cx.debug_bounds("code").unwrap();
    let terminal = cx.debug_bounds("terminal").unwrap();
    assert!(cx.debug_bounds("code-resize").is_some());
    assert!(cx.debug_bounds("code-placeholder").is_some());
    // The page fills the column inside its 1 px line and draws above GPUI,
    // so the edge to drag covers the line and hangs outside it, over the
    // Herdr side, ending where the page begins.
    let grip = cx.debug_bounds("code-resize").unwrap();
    assert_eq!(grip.right(), panel.left() + px(1.));
    assert!(grip.left() < panel.left());
    assert!(
        terminal.right() <= panel.left(),
        "the panel sits to the right"
    );
    // It is not a tab: the strip lists none, and nothing was opened.
    assert!(cx.debug_bounds("browser-tab-0").is_none());
    assert!(cx.update(|_, cx| {
        cx.try_global::<Store>().is_none_or(|store| {
            store
                .code_tab(&scope(&view.read(cx).endpoints[0]), "w0")
                .is_none()
        })
    }));

    // Another workspace has its own panel, hidden until shown there.
    focus_workspace(&view, cx, "w1");
    assert!(cx.debug_bounds("code").is_none());
    focus_workspace(&view, cx, "w0");
    assert!(cx.debug_bounds("code").is_some());

    // Dragging its edge leftward widens it, for every workspace of the
    // window, and the width is kept once the drag ends.
    let edge = cx.debug_bounds("code-resize").unwrap();
    let start = edge.center();
    let to = point(start.x - px(60.), start.y);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        point(start.x - px(20.), start.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    draw(cx);
    let wider = cx.debug_bounds("code").unwrap();
    assert!(
        (wider.size.width - panel.size.width - px(60.)).abs() <= px(4.),
        "{panel:?} {wider:?}"
    );
    view.update(cx, |view, _| {
        assert!(view.code_width.chosen().is_some());
        assert!(!view.code_width.take_unsaved(), "saved on release");
    });

    run(&view, cx, Command::ToggleCode);
    assert!(cx.debug_bounds("code").is_none());
    assert!(cx.debug_bounds("code-resize").is_none());
    assert!(cx.debug_bounds("terminal").is_some());
}

/// A page that could not be created is not retried every tick, and another
/// tab's failure must not make it forget that.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_failed_vs_code_page_stays_failed_when_another_page_fails(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let tab = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        view.update(cx, |view, _| {
            view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/").unwrap());
            view.browser.code_server.probe = answers;
        });
        Store::update(cx, |store| {
            store.open_code_tab(tab_scope, "w0", url("http://127.0.0.1:8000/"))
        })
        .unwrap()
    });
    // The window takes the address, which starts its pages over, and the
    // server answers.
    run(&view, cx, Command::ToggleCode);
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.browser.failed.insert(tab, "no web view".into());
            // A strip tab fails after it.
            let other = crate::browser::TabId::test(999);
            view.browser
                .failed
                .insert(other, "no web view either".into());
        })
    });
    // The server answers, so only the failure keeps the page from retrying.
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    view.read_with(cx, |view, _| {
        assert!(matches!(
            view.browser.code_server.state,
            Reach::Ready { .. }
        ));
    });
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.browser
                .failed
                .get(&tab)
                .map(|message| message.as_ref()),
            Some("no web view")
        );
        #[cfg(any(target_os = "macos", windows))]
        assert!(!view.browser.pages.contains(tab), "not retried");
    });
    assert!(cx.debug_bounds("code-placeholder").is_some());
}

// Only builds that show pages ask the server, so only they need answers.
#[cfg(any(target_os = "macos", windows))]
const COMMIT: &str = "2a59476c9bfcb90b3ddc372c36762471b7dfad1c";

#[cfg(any(target_os = "macos", windows))]
fn answers(_: &WebUrl) -> crate::Result<Server> {
    Ok(Server::from_version(COMMIT).unwrap())
}

fn refuses(_: &WebUrl) -> crate::Result<Server> {
    Err(crate::Error::CodeTokenRefused)
}

/// Sets the server and how it answers, and shows the panel.
fn show_with(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    address: &str,
    probe: fn(&WebUrl) -> crate::Result<Server>,
) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.config.code.url = Some(WebUrl::try_from(address).unwrap());
            view.browser.code_server.probe = probe;
        })
    });
    run(view, cx, Command::ToggleCode);
    cx.run_until_parked();
    draw(cx);
}

/// A server that does not answer is named in the panel, and asked again
/// once a while has passed.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn an_unreachable_server_is_shown_and_asked_again(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    show_with(&view, cx, "http://127.0.0.1:8000/?tkn=x", refuses);
    assert!(cx.debug_bounds("code-unreachable").is_some());
    assert!(cx.debug_bounds("code-placeholder").is_none());
    view.read_with(cx, |view, _| {
        let Reach::Failed { message, .. } = &view.browser.code_server.state else {
            panic!("{:?}", view.browser.code_server.state);
        };
        assert!(message.contains("connection token"), "{message}");
    });

    // Too soon: not asked again.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.browser.code_server.probe = answers;
            view.ensure_code_page(window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(matches!(
            view.browser.code_server.state,
            Reach::Failed { .. }
        ));
    });

    // Once the wait is over, the server is asked again and answers.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let Reach::Failed { at, .. } = &mut view.browser.code_server.state else {
                panic!("failed");
            };
            *at = std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(6))
                .unwrap();
            view.ensure_code_page(window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let Reach::Ready { server, .. } = &view.browser.code_server.state else {
            panic!("{:?}", view.browser.code_server.state);
        };
        assert_eq!(server, &Server::from_version(COMMIT).unwrap());
    });
}

/// A new server address closes the pages still on the old one, so each
/// workspace reopens VS Code on the new server.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_new_server_address_closes_pages_on_the_old_one(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let old = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            store.open_code_tab(tab_scope, "w1", url("http://127.0.0.1:8000/?folder=/x"))
        })
        .unwrap()
    });
    // The pages are not created: the server never answers here.
    show_with(&view, cx, "http://127.0.0.1:9000/?tkn=y", refuses);
    cx.update(|_, cx| assert!(cx.global::<Store>().get(old).is_none()));
}

/// A new token for the same server keeps each workspace's tab, and with it
/// the folder the page went to; only its page is created anew.
#[gpui::test]
fn a_new_token_on_the_same_server_keeps_the_tabs(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let kept = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            store.open_code_tab(tab_scope, "w1", url("http://127.0.0.1:8000/?folder=/x"))
        })
        .unwrap()
    });
    show_with(&view, cx, "http://127.0.0.1:8000/?tkn=new", refuses);
    cx.update(|_, cx| {
        assert_eq!(
            cx.global::<Store>().get(kept).unwrap().location,
            Some(url("http://127.0.0.1:8000/?folder=/x"))
        );
    });
}

/// An answer is trusted only for a while: once the server stops, a panel
/// opened in another space asks it again and says it cannot be reached,
/// rather than showing a page that stays blank.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_new_panel_asks_the_server_again_once_an_answer_is_old(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    show_with(&view, cx, "http://127.0.0.1:8000/?tkn=x", answers);
    view.update(cx, |view, _| {
        let Reach::Ready { at, .. } = &mut view.browser.code_server.state else {
            panic!("{:?}", view.browser.code_server.state);
        };
        *at = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(6))
            .unwrap();
        // The server stops.
        view.browser.code_server.probe = refuses;
    });
    focus_workspace(&view, cx, "w1");
    run(&view, cx, Command::ToggleCode);
    cx.run_until_parked();
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(
            matches!(view.browser.code_server.state, Reach::Failed { .. }),
            "{:?}",
            view.browser.code_server.state
        );
    });
    assert!(cx.debug_bounds("code-unreachable").is_some());
}

/// Creates the focused workspace's page, as a tick does once the server
/// has answered; a headless window may record why it could not instead.
#[cfg(any(target_os = "macos", windows))]
fn create_page(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> crate::browser::TabId {
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    draw(cx);
    cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        let workspace = view.read(cx).browser_key().unwrap().1;
        cx.global::<Store>()
            .code_tab(&tab_scope, &workspace)
            .unwrap()
            .id
    })
}

/// An answer serves one page: once the server stops, a panel opened in
/// another space right away asks it again and says it cannot be reached,
/// rather than showing a page that stays blank.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn each_new_panel_asks_the_server_again(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    show_with(&view, cx, "http://127.0.0.1:8000/?tkn=x", answers);
    create_page(&view, cx);
    view.update(cx, |view, _| {
        assert_eq!(view.browser.code_server.state, Reach::Unknown);
        // The server stops.
        view.browser.code_server.probe = refuses;
    });
    focus_workspace(&view, cx, "w1");
    run(&view, cx, Command::ToggleCode);
    cx.run_until_parked();
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(
            matches!(view.browser.code_server.state, Reach::Failed { .. }),
            "{:?}",
            view.browser.code_server.state
        );
    });
    assert!(cx.debug_bounds("code-unreachable").is_some());
}

/// Removing the server's address closes the pages still showing it, so the
/// panel asks for an address instead.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn removing_the_address_closes_the_panel_pages(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    show_with(&view, cx, "http://127.0.0.1:8000/?tkn=x", answers);
    let tab = create_page(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.code.url = None;
            view.ensure_code_page(window, cx);
        })
    });
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.browser.pages.contains(tab));
        assert!(!view.browser.failed.contains_key(&tab));
        assert_eq!(view.browser.code_server.state, Reach::Unknown);
    });
    assert!(cx.debug_bounds("code-placeholder").is_some());
}
