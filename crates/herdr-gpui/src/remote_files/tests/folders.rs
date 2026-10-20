//! Folder clicks: the probe, the questions a large or changed tree asks, the
//! archive fetch and extraction, and the copy that opens.

use super::*;
use crate::menu::Page;
use gpui::Modifiers;
use herdr_client::{ProbedFolder, RemoteFileError};
use std::io::Write;

const PATH: &str = "/home/me/src";

/// A host whose path is a folder, so a file fetch refuses it.
fn not_a_file() -> herdr_client::Result<&'static [u8]> {
    Err(herdr_client::Error::RemoteFile(RemoteFileError::NotAFile))
}

/// A tree `bytes` large with fingerprint `fingerprint`, archived under `src`.
fn sized(bytes: u64, fingerprint: Option<[u8; 32]>) -> herdr_client::Result<ProbedFolder> {
    Ok(ProbedFolder {
        bytes,
        fingerprint,
        top: "src".into(),
        links_skipped: 0,
    })
}

/// A tree whose host archive leaves `count` links out.
fn with_links(count: u32) -> herdr_client::Result<ProbedFolder> {
    Ok(ProbedFolder {
        bytes: 4096,
        fingerprint: Some([7; 32]),
        top: "src".into(),
        links_skipped: count,
    })
}

fn two_links_skipped() -> herdr_client::Result<ProbedFolder> {
    with_links(2)
}

fn one_link_skipped() -> herdr_client::Result<ProbedFolder> {
    with_links(1)
}

fn probed() -> herdr_client::Result<ProbedFolder> {
    sized(4096, Some([7; 32]))
}

fn changed() -> herdr_client::Result<ProbedFolder> {
    sized(4096, Some([9; 32]))
}

fn large() -> herdr_client::Result<ProbedFolder> {
    sized(2 * 1024 * 1024, Some([7; 32]))
}

fn probe_fails() -> herdr_client::Result<ProbedFolder> {
    Err(herdr_client::Error::RemoteFile(RemoteFileError::Missing))
}

fn not_a_folder() -> herdr_client::Result<ProbedFolder> {
    Err(herdr_client::Error::RemoteFile(RemoteFileError::NotAFolder))
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (path, bytes) in entries {
        writer.start_file(*path, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// A gzipped tar holding `notes.md` and a symbolic link to it.
fn tar_gz_with_link() -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut file = tar::Header::new_gnu();
    file.set_size(4);
    file.set_mode(0o644);
    file.set_cksum();
    builder
        .append_data(&mut file, "src/notes.md", &b"note"[..])
        .unwrap();
    let mut link = tar::Header::new_gnu();
    link.set_entry_type(tar::EntryType::Symlink);
    link.set_size(0);
    link.set_mode(0o777);
    link.set_link_name("notes.md").unwrap();
    link.set_cksum();
    builder
        .append_data(&mut link, "src/link", std::io::empty())
        .unwrap();
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&builder.into_inner().unwrap()).unwrap();
    encoder.finish().unwrap()
}

/// A click on `path` in a pane on the fake host, and the work it starts.
fn click(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, path: &str) {
    view.update(cx, |view, cx| view.open_remote_file(remote(path), None, cx));
    cx.run_until_parked();
}

/// One poll of the window, and its work finished.
fn poll(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        view.remote_files.last_watch = None;
        view.poll_remote_files(window, cx);
    });
    cx.run_until_parked();
}

fn page(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Page> {
    view.read_with(cx, |view, _| view.menu.page)
}

fn press(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let button = cx.debug_bounds(id).unwrap().center();
    cx.simulate_click(button, Modifiers::default());
    cx.run_until_parked();
}

fn flash_text(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| {
        view.flash.as_ref().map(|(flash, _)| flash.text.to_string())
    })
}

/// A window on `host` whose remote folder path is the one clicked.
fn folder_window<'a>(
    host: &FakeHost,
    warning_mb: u32,
    cx: &'a mut TestAppContext,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let (view, cx) = window(host, cx);
    view.update(cx, |view, _| {
        host.serve(&mut view.remote_files, not_a_file);
        view.config.remote_files.folder_warning_mb = warning_mb;
    });
    (view, cx)
}

#[gpui::test]
fn a_click_on_a_remote_folder_copies_and_opens_it(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&zip_of(&[("src/notes.md", b"notes")]));
    click(&view, cx, PATH);
    assert_eq!(host.probes().len(), 1);
    assert_eq!(host.archives().len(), 1);
    let (local, fingerprint) = view
        .read_with(cx, |view, _| view.remote_files.folder("me@devbox", PATH))
        .expect("the folder is copied");
    assert_eq!(fingerprint, Some([7; 32]));
    assert_eq!(std::fs::read(local.join("notes.md")).unwrap(), b"notes");
    let opened = cx.opened_url().expect("the copy's folder opens");
    let opened = url::Url::parse(&opened).unwrap().to_file_path().unwrap();
    // `opened` has symlinks resolved (macOS keeps temporary folders behind
    // `/var -> /private/var`), so compare both sides resolved.
    assert_eq!(
        opened.canonicalize().unwrap(),
        local.canonicalize().unwrap(),
        "the copy's folder URL opens"
    );
    view.read_with(cx, |view, _| {
        let listed: Vec<_> = view.remote_files.list().collect();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].state, SyncState::Synced);
    });
}

#[gpui::test]
fn a_folder_over_the_warning_size_asks_and_transfer_anyway_copies(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 1, cx);
    host.serve_archive(&zip_of(&[("src/a.txt", b"a")]));
    view.update(cx, |_, _| host.answer_probes(&[large]));
    click(&view, cx, PATH);
    assert!(host.archives().is_empty(), "nothing was copied yet");
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteFolder));
    view.read_with(cx, |view, _| {
        let prompt = view
            .menu
            .remote_folder
            .as_ref()
            .expect("the question is stored");
        assert_eq!(
            prompt.kind,
            FolderQuestion::TooLarge {
                bytes: 2 * 1024 * 1024
            }
        );
    });
    press(cx, "remote-folder-transfer");
    assert_eq!(host.archives().len(), 1);
    assert_eq!(page(&view, cx), None);
    assert!(
        view.read_with(cx, |view, _| view
            .remote_files
            .folder("me@devbox", PATH)
            .is_some()),
        "the folder is copied"
    );
}

#[gpui::test]
fn cancelling_a_large_folder_transfers_nothing_and_lets_a_click_try_again(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 1, cx);
    host.serve_archive(&zip_of(&[("src/a.txt", b"a")]));
    view.update(cx, |_, _| host.answer_probes(&[large, large]));
    click(&view, cx, PATH);
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteFolder));
    press(cx, "remote-folder-cancel");
    assert!(host.archives().is_empty(), "Cancel copies nothing");
    assert!(
        view.read_with(cx, |view, _| view
            .remote_files
            .folder("me@devbox", PATH)
            .is_none()),
        "no copy is left"
    );
    click(&view, cx, PATH);
    assert_eq!(host.probes().len(), 2, "a second click probes again");
}

#[gpui::test]
fn a_repeat_click_on_an_unchanged_folder_opens_the_copy_without_fetching(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&zip_of(&[("src/notes.md", b"notes")]));
    click(&view, cx, PATH);
    cx.update(|_, cx| cx.open_url("about:blank"));
    click(&view, cx, PATH);
    assert_eq!(host.probes().len(), 2, "the second click probes again");
    assert_eq!(host.archives().len(), 1, "the archive is fetched once");
    let opened = cx.opened_url().expect("the copy opens again");
    assert!(opened.as_str().ends_with("/src"), "{opened}");
}

#[gpui::test]
fn a_repeat_click_on_a_changed_folder_asks_and_refetches(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&zip_of(&[("src/notes.md", b"old")]));
    view.update(cx, |_, _| host.answer_probes(&[probed, changed]));
    click(&view, cx, PATH);
    host.serve_archive(&zip_of(&[("src/notes.md", b"new")]));
    click(&view, cx, PATH);
    assert_eq!(host.archives().len(), 1, "the changed folder asks first");
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteFolder));
    view.read_with(cx, |view, _| {
        let prompt = view
            .menu
            .remote_folder
            .as_ref()
            .expect("the question is stored");
        assert_eq!(prompt.kind, FolderQuestion::Changed { bytes: 4096 });
    });
    press(cx, "remote-folder-refetch");
    assert_eq!(host.archives().len(), 2);
    let (local, fingerprint) = view
        .read_with(cx, |view, _| view.remote_files.folder("me@devbox", PATH))
        .unwrap();
    assert_eq!(fingerprint, Some([9; 32]));
    assert_eq!(std::fs::read(local.join("notes.md")).unwrap(), b"new");
}

#[gpui::test]
fn opening_an_existing_folder_after_a_change_fetches_nothing(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&zip_of(&[("src/notes.md", b"old")]));
    view.update(cx, |_, _| host.answer_probes(&[probed, changed]));
    click(&view, cx, PATH);
    cx.update(|_, cx| cx.open_url("about:blank"));
    click(&view, cx, PATH);
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteFolder));
    press(cx, "remote-folder-open");
    assert_eq!(host.archives().len(), 1, "opening fetches nothing");
    let opened = cx.opened_url().expect("the existing copy opens");
    assert!(opened.as_str().ends_with("/src"), "{opened}");
    assert_eq!(page(&view, cx), None);
}

#[gpui::test]
fn a_folder_over_the_cap_flashes_and_copies_nothing(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 1, cx);
    view.update(cx, |view, _| {
        view.config.remote_files.folder_transfer_cap_mb = 1;
        host.answer_probes(&[probed, probed]);
    });
    // The archive is over the cap, so the host refuses it before sending.
    let archive = vec![0; 2 * 1024 * 1024];
    host.serve_archive(&archive);
    click(&view, cx, PATH);
    assert_eq!(host.archives().len(), 1, "the archive fetch was asked");
    let text = flash_text(&view, cx).expect("the cap is reported");
    assert!(text.contains("larger than 1 MB"), "{text}");
    assert!(
        view.read_with(cx, |view, _| view
            .remote_files
            .folder("me@devbox", PATH)
            .is_none()),
        "nothing is copied"
    );
    click(&view, cx, PATH);
    assert_eq!(host.probes().len(), 2, "the key was cleared");
}

#[gpui::test]
fn a_probe_error_flashes_and_lets_the_next_click_probe_again(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&zip_of(&[("src/a.txt", b"a")]));
    view.update(cx, |_, _| host.answer_probes(&[probe_fails, probed]));
    click(&view, cx, PATH);
    assert!(host.archives().is_empty());
    let text = flash_text(&view, cx).expect("the probe error is reported");
    assert!(text.contains("Could not check the folder"), "{text}");
    assert!(
        view.read_with(cx, |view, _| view
            .remote_files
            .folder("me@devbox", PATH)
            .is_none()),
        "nothing is copied"
    );
    click(&view, cx, PATH);
    assert_eq!(host.probes().len(), 2, "the key was cleared");
    assert_eq!(host.archives().len(), 1);
}

#[gpui::test]
fn links_the_extraction_skipped_are_counted_and_flashed(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&tar_gz_with_link());
    let posix = RemotePath {
        target: "me@devbox".into(),
        shell: RemoteShell::Posix,
        path: PATH.into(),
        line: None,
    };
    view.update(cx, |view, cx| view.open_remote_file(posix, None, cx));
    cx.run_until_parked();
    let text = flash_text(&view, cx).expect("the skipped link is reported");
    assert_eq!(text, "1 link was skipped");
    let (local, _) = view
        .read_with(cx, |view, _| view.remote_files.folder("me@devbox", PATH))
        .unwrap();
    assert_eq!(std::fs::read(local.join("notes.md")).unwrap(), b"note");
    assert!(!local.join("link").exists(), "no link is created");
}

#[gpui::test]
fn a_click_on_a_path_with_a_trailing_separator_opens_the_copy(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&zip_of(&[("src/notes.md", b"notes")]));
    click(&view, cx, "/home/me/src/");
    let (local, _) = view
        .read_with(cx, |view, _| {
            view.remote_files.folder("me@devbox", "/home/me/src/")
        })
        .expect("the folder is copied");
    assert_eq!(local.file_name().unwrap(), "src");
    let opened = cx.opened_url().expect("the copy's folder opens");
    let opened = url::Url::parse(&opened).unwrap().to_file_path().unwrap();
    assert_eq!(
        opened.canonicalize().unwrap(),
        local.canonicalize().unwrap()
    );
}

#[gpui::test]
fn links_the_host_skipped_are_counted_and_flashed(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&zip_of(&[("src/notes.md", b"notes")]));
    view.update(cx, |_, _| host.answer_probes(&[two_links_skipped]));
    click(&view, cx, PATH);
    let text = flash_text(&view, cx).expect("the host's skipped links are reported");
    assert_eq!(text, "2 links were skipped");

    let single = FakeHost::new();
    let (view, cx) = folder_window(&single, 64, cx);
    single.serve_archive(&zip_of(&[("src/notes.md", b"notes")]));
    view.update(cx, |_, _| single.answer_probes(&[one_link_skipped]));
    click(&view, cx, PATH);
    let text = flash_text(&view, cx).expect("the host's skipped link is reported");
    assert_eq!(text, "1 link was skipped");
}

#[gpui::test]
fn a_folder_that_became_a_file_is_forgotten_and_the_next_click_fetches_it(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&zip_of(&[("src/notes.md", b"notes")]));
    view.update(cx, |_, _| host.answer_probes(&[probed, not_a_folder]));
    click(&view, cx, PATH);
    assert!(
        view.read_with(cx, |view, _| view
            .remote_files
            .folder("me@devbox", PATH)
            .is_some()),
        "the folder is copied first"
    );
    click(&view, cx, PATH);
    let text = flash_text(&view, cx).expect("the change is reported");
    assert!(text.contains("Could not check the folder"), "{text}");
    assert!(
        view.read_with(cx, |view, _| view
            .remote_files
            .copy("me@devbox", PATH)
            .is_none()),
        "the folder copy is forgotten"
    );
    host.answer_fetches(&[notes]);
    click(&view, cx, PATH);
    let copy = view
        .read_with(cx, |view, _| {
            view.remote_files.copy("me@devbox", PATH).cloned()
        })
        .expect("the file is fetched");
    assert_eq!(copy.kind, CopyKind::File);
    assert_eq!(std::fs::read(&copy.local).unwrap(), NOTES);
}

#[gpui::test]
fn a_file_that_became_a_folder_is_replaced_by_the_folder(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = window(&host, cx);
    click(&view, cx, PATH);
    let local = view.read_with(cx, |view, _| {
        view.remote_files
            .copy("me@devbox", PATH)
            .unwrap()
            .local
            .clone()
    });
    assert!(local.is_file(), "a file copy is here");
    host.answer_fetches(&[not_a_file]);
    view.update(cx, |_, _| host.answer_probes(&[probed]));
    host.serve_archive(&zip_of(&[("src/inner.txt", b"inner")]));
    click(&view, cx, PATH);
    let (folder, _) = view
        .read_with(cx, |view, _| view.remote_files.folder("me@devbox", PATH))
        .expect("the folder is copied");
    assert_eq!(folder, local, "the same path now holds a folder");
    assert!(folder.is_dir());
    assert_eq!(std::fs::read(folder.join("inner.txt")).unwrap(), b"inner");
}

#[gpui::test]
fn a_folder_question_waits_while_another_page_is_open(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 1, cx);
    host.serve_archive(&zip_of(&[("src/a.txt", b"a")]));
    view.update(cx, |_, _| host.answer_probes(&[large]));
    click(&view, cx, PATH);
    view.update_in(cx, |view, window, cx| {
        assert!(view.open_menu(window, cx), "the menu opens");
    });
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::Menu), "the question waits");
    view.update_in(cx, |view, window, cx| view.dismiss_menu(window, cx));
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteFolder));
}

#[gpui::test]
fn quitting_removes_the_folder_copy(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = folder_window(&host, 64, cx);
    host.serve_archive(&zip_of(&[("src/notes.md", b"notes")]));
    click(&view, cx, PATH);
    let (local, _) = view
        .read_with(cx, |view, _| view.remote_files.folder("me@devbox", PATH))
        .unwrap();
    assert!(local.is_dir());
    view.update(cx, |view, cx| view.remote_files.remove_copies(cx))
        .detach();
    cx.run_until_parked();
    assert!(!local.exists(), "the copy is removed with the cache");
}
