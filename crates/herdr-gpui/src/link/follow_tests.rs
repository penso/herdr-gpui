#![allow(clippy::unwrap_used)]

use super::*;
use herdr_client::protocol::ClientShellPane;

/// A snapshot whose workspace `w0` was opened in `root`.
fn opened_in(root: &Path) -> ClientShellSnapshot {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(2);
    snapshot.panes.push(ClientShellPane {
        pane_id: "p0".into(),
        workspace_id: "w0".into(),
        tab_id: "t0".into(),
        label: None,
        cwd: Some(root.to_string_lossy().into_owned()),
        foreground_cwd: None,
        focused: true,
        right_click_passthrough: false,
    });
    snapshot
}

#[test]
fn a_checkout_open_in_a_workspace_is_found_however_it_is_spelled() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = dir.path().join("repo");
    std::fs::create_dir_all(checkout.join(".git")).unwrap();
    let snapshot = opened_in(&checkout);
    let found = checkout_at(&checkout.join("."), &snapshot).unwrap();
    assert_eq!(found.workspace.as_deref(), Some("w0"));
    assert_eq!(found.path, canonical(&checkout).unwrap());
    // Plain path, as Herdr reports it; never the `\\?\` form.
    assert!(!found.path.to_string_lossy().starts_with(r"\\"));
}

#[test]
fn a_linked_worktree_with_a_git_file_is_a_checkout_too() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".git"), "gitdir: elsewhere").unwrap();
    let found = checkout_at(dir.path(), &opened_in(Path::new("/nowhere"))).unwrap();
    assert_eq!(found.workspace, None);
}

#[test]
fn folders_that_are_not_checkouts_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let snapshot = opened_in(dir.path());
    assert!(matches!(
        checkout_at(dir.path(), &snapshot),
        Err(Error::NotCheckout(_))
    ));
    let file = dir.path().join("file");
    std::fs::write(&file, "").unwrap();
    assert!(matches!(
        checkout_at(&file, &snapshot),
        Err(Error::NotFolder(_))
    ));
    assert!(matches!(
        checkout_at(&dir.path().join("missing"), &snapshot),
        Err(Error::Unreadable { .. })
    ));
}

#[gpui::test]
fn a_device_link_waits_until_the_saved_devices_are_read(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        assert!(!view.read(cx).catalog.loaded());
        // A window starts with Local alone; a saved device is not there yet.
        defer(
            Link::Device {
                id: "ssh:later".into(),
            },
            cx,
        );
        view.update(cx, |view, cx| poll(view, window, cx));
        assert!(cx.global::<Pending>().0.is_some());
        // One the window already has is followed at once.
        defer(Link::Device { id: LOCAL.into() }, cx);
        view.update(cx, |view, cx| poll(view, window, cx));
        assert!(cx.global::<Pending>().0.is_none());
    });
}

const REFUSALS: [Refusal; 5] = [
    Refusal::Closed,
    Refusal::DialogOpen,
    Refusal::NoDevice(String::new()),
    Refusal::NotConnected,
    Refusal::NotificationGone,
];

/// The command line keeps its English, without the window's isolation
/// marks, whatever language the window shows.
#[test]
fn a_refused_link_answers_the_command_line_in_english() {
    let message = |refusal: Refusal| match refusal.response() {
        Response::Error { message, .. } => message,
        other => panic!("not an error: {other:?}"),
    };
    crate::i18n::leak_tests::under_en_xa("link refusals in English", |_, _| {
        assert_eq!(message(Refusal::NoDevice("nas".into())), "No device nas");
        assert_eq!(
            REFUSALS.map(message),
            [
                "The window closed",
                "A dialog is open in the window; close it first",
                "No device ",
                "Herdr UI is not connected to this computer's Herdr",
                "That notification is no longer known",
            ]
        );
    });
}

/// The window says why in its own language.
#[test]
fn a_refused_link_tells_the_window_in_messages() {
    crate::i18n::leak_tests::under_en_xa("link refusals en-XA leak", |_, _| {
        for refusal in REFUSALS {
            let shown = crate::i18n::fl!("link-open-failed", reason = refusal.shown());
            assert!(
                herdr_i18n::pseudo::is_marked(&shown),
                "{refusal:?}: {shown}"
            );
        }
        assert!(herdr_i18n::pseudo::is_marked(&crate::i18n::fl!(
            "link-open-dialog-first"
        )));
    });
}

/// Each window clears only its own link's complaint: another window's
/// later complaint must not leave this one's on its status line.
#[gpui::test]
fn each_window_clears_its_own_link_complaint(cx: &mut gpui::TestAppContext) {
    let refuse = |name: &str| Err(Error::NotFolder(PathBuf::from(name)));
    let first = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let second = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let shown = |handle: WindowHandle<HerdrWindow>, cx: &mut gpui::TestAppContext| {
        handle
            .update(cx, |view, _, _| view.local_error.clone())
            .unwrap()
    };
    first
        .update(cx, |view, window, cx| {
            view.show_checkout(refuse("/first"), window, cx);
        })
        .unwrap();
    second
        .update(cx, |view, window, cx| {
            view.show_checkout(refuse("/second"), window, cx);
        })
        .unwrap();
    let complaint = shown(first, cx).unwrap();
    assert!(complaint.contains("/first"), "{complaint}");

    first
        .update(cx, |view, window, cx| {
            let found = Ok(Checkout {
                path: PathBuf::from("/checkout"),
                workspace: Some("w0".into()),
            });
            view.show_checkout(found, window, cx);
        })
        .unwrap();
    assert_eq!(shown(first, cx), None);
    // The other window's complaint is its own to clear.
    let other = shown(second, cx).unwrap();
    assert!(other.contains("/second"), "{other}");
}
