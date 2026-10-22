#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    browser::{LocalFile, OrchestratorRepo, Scope, WebUrl},
    code_view::CodeFile,
    control::protocol::Request,
    sidebar::layout_tests::{fixture_window, snapshot},
};
use gpui::{Entity, VisualTestContext};
use std::{path::Path, sync::Arc};

mod px0;
mod review;
mod windows;

fn web(url: &str) -> Location {
    Location::Web {
        url: WebUrl::try_from(url).unwrap(),
    }
}

#[test]
fn tabs_are_listed_by_kind_with_what_they_show() {
    let scope = Scope::local(Path::new("/tmp/herdr.sock"));
    let mut store = Store::default();
    // Hidden folders are never served, so the folder has a plain name.
    let folder = tempfile::Builder::new().prefix("page").tempdir().unwrap();
    let page = folder.path().join("index.html");
    std::fs::write(&page, "<p>hi</p>").unwrap();
    let locations = [
        None,
        Some(web("https://a.test/docs")),
        Some(Location::Local {
            file: LocalFile::new(&page, None).unwrap(),
        }),
        Some(Location::Code {
            file: CodeFile {
                path: "/src/main.rs".into(),
                line: Some(4),
            },
        }),
        Some(Location::Orchestrator {
            repo: OrchestratorRepo::new("/src/app".into()).unwrap(),
        }),
        Some(Location::Devices),
    ];
    for location in locations {
        store.open(scope.clone(), "w_1", location, Some("w_1:p1".into()));
    }
    // The VS Code tab's address carries its token, which is never listed.
    store.open_editor_tab(
        scope.clone(),
        "w_1",
        crate::browser::Editor::VsCode,
        Some(web("http://127.0.0.1:8000/?tkn=secret")),
    );
    let listed: Vec<TabInfo> = store.tabs().map(info).collect();
    let kinds: Vec<TabKind> = listed.iter().map(|tab| tab.kind).collect();
    assert_eq!(
        kinds,
        [
            TabKind::Blank,
            TabKind::Web,
            TabKind::File,
            TabKind::Code,
            TabKind::Orchestrator,
            TabKind::Devices,
            TabKind::Editor,
        ]
    );
    let locations: Vec<Option<&str>> = listed.iter().map(|tab| tab.location.as_deref()).collect();
    assert_eq!(locations[0], None);
    assert_eq!(locations[1], Some("https://a.test/docs"));
    assert!(locations[2].unwrap().ends_with("index.html"));
    assert_eq!(locations[3], Some("/src/main.rs"));
    assert_eq!(locations[4], Some("/src/app"));
    assert_eq!(locations[5], None);
    assert_eq!(locations[6], None);
    let first = &listed[1];
    assert_eq!(first.workspace, "w_1");
    assert_eq!(first.host, "local:/tmp/herdr.sock");
    assert_eq!(first.opened_by.as_deref(), Some("w_1:p1"));
    assert_eq!(first.title, "a.test");
}

/// The fixture window, showing workspace `w0` of two as a connected daemon
/// would.
fn window(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(2);
        shown.focused_workspace_id = Some("w0".into());
        shown.focused_tab_id = Some("t0".into());
        view.live.snapshot = Some(Arc::new(shown));
        view
    })
}

/// Opens a tab in workspace `workspace` of the window's own daemon.
fn open(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    workspace: &str,
    location: Option<Location>,
) -> TabId {
    cx.cx.update(|cx| {
        let scope = crate::browser::scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| store.open(scope, workspace, location, None)).unwrap()
    })
}

/// A request queued on a private socket, and the caller waiting for it.
fn queued(
    server: &socket::Server,
    op: TabOp,
    caller: Caller,
) -> (
    Call<TabOp>,
    socket::Incoming,
    std::thread::JoinHandle<crate::Result<Response>>,
) {
    let path = server.path().to_owned();
    let call = Call { caller, op };
    let request = Request::Tab(call.clone());
    let waiting = std::thread::spawn(move || socket::call(&path, &request));
    (call, server.next().unwrap(), waiting)
}
