//! Clicks on paths a pane on an SSH host printed, fetched from a fake host.

use super::*;
use crate::{
    remote_files::tests::{FakeHost, NOTES, missing, notes},
    window::{RemotePath, flash::Tone},
};
use herdr_client::RemoteShell;

pub(super) const TARGET: &str = "me@devbox";

/// A window whose SSH pane prints `text` in `cwd`, fetching from `host`.
pub(super) fn on_host<'a>(
    host: &FakeHost,
    reply: fn() -> herdr_client::Result<&'static [u8]>,
    text: &str,
    cwd: &str,
    cx: &'a mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let (view, cx) = window(text, Path::new(cwd), cx);
    view.update(cx, |view, _| {
        view.endpoints[view.selected_endpoint].connection.target = ConnectTarget::Ssh {
            target: TARGET.into(),
            session: "default".into(),
        };
        host.serve(&mut view.remote_files, reply);
    });
    (view, cx)
}

fn asked(shell: RemoteShell, path: &str, line: Option<u32>) -> Vec<RemotePath> {
    vec![RemotePath {
        target: TARGET.into(),
        shell,
        path: path.into(),
        line,
    }]
}

#[gpui::test]
fn a_click_on_a_printed_drive_path_fetches_it_and_opens_the_copy(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_host(&host, notes, r"see C:\dev\notes.md:3 now", r"C:\dev", cx);
    let path_cell = at(&view, cx, 8);
    modifier_click(cx, path_cell, Modifiers::secondary_key());

    let path = r"C:\dev\notes.md";
    assert_eq!(host.asked(), asked(RemoteShell::PowerShell, path, Some(3)));
    let opened = cx.opened_url().unwrap();
    assert!(
        opened.starts_with("file://") && opened.ends_with("/notes.md"),
        "{opened}"
    );
    view.read_with(cx, |view, _| {
        let copy = view.remote_files.copy(TARGET, path).unwrap();
        assert_eq!(std::fs::read(&copy.local).unwrap(), NOTES);
        assert_eq!(copy.shell, RemoteShell::PowerShell);
        let real = real_path(&copy.local).unwrap();
        assert_eq!(opened, url::Url::from_file_path(real).unwrap().as_str());
        let (flash, _) = view.flash.as_ref().unwrap();
        assert_eq!(flash.text.as_ref(), "Fetching notes.md from me@devbox…");
    });

    // Another click fetches the host's file again into the same copy.
    let path_cell = at(&view, cx, 8);
    modifier_click(cx, path_cell, Modifiers::secondary_key());
    assert_eq!(host.asked().len(), 2);
    assert!(cx.opened_url().unwrap().ends_with("/notes.md"));
}

#[gpui::test]
fn a_relative_path_is_asked_for_under_the_pane_s_windows_directory(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_host(&host, notes, r"open src\main.rs", r"C:\dev\app", cx);
    let path_cell = at(&view, cx, 7);
    modifier_click(cx, path_cell, Modifiers::secondary_key());
    assert_eq!(
        host.asked(),
        asked(RemoteShell::PowerShell, r"C:\dev\app\src\main.rs", None)
    );
    assert!(cx.opened_url().unwrap().ends_with("/main.rs"));
}

#[gpui::test]
fn a_home_path_on_a_posix_host_is_left_for_it_to_expand(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_host(&host, notes, "open ~/x.md", "/home/me", cx);
    let path_cell = at(&view, cx, 7);
    modifier_click(cx, path_cell, Modifiers::secondary_key());
    assert_eq!(host.asked(), asked(RemoteShell::Posix, "~/x.md", None));
    assert!(cx.opened_url().unwrap().ends_with("/x.md"));
}

#[gpui::test]
fn a_failed_fetch_says_why_and_opens_nothing(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_host(&host, missing, "open /home/me/gone.md", "/home/me", cx);
    let path_cell = at(&view, cx, 7);
    modifier_click(cx, path_cell, Modifiers::secondary_key());
    assert_eq!(host.asked().len(), 1);
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| {
        let (flash, _) = view.flash.as_ref().unwrap();
        assert_eq!(flash.tone, Tone::Warning);
        assert_eq!(
            flash.text.as_ref(),
            "Could not fetch the file: the file is not on the host"
        );
        assert!(view.remote_files.copy(TARGET, "/home/me/gone.md").is_none());
    });
}
