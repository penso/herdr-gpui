//! A new VS Code page opens on the folder its workspace started in.
use super::*;
use crate::{code_server::Server, controls::Command};

const COMMIT: &str = "2a59476c9bfcb90b3ddc372c36762471b7dfad1c";

/// The workspace's folder, which must be absolute where the test runs, and
/// how it reads in the address.
#[cfg(not(windows))]
const FOLDER: (&str, &str) = ("/Users/me/project", "%2FUsers%2Fme%2Fproject");
#[cfg(windows)]
const FOLDER: (&str, &str) = (r"C:\Users\me\project", "C%3A%5CUsers%5Cme%5Cproject");

fn answers(_: &WebUrl) -> crate::Result<Server> {
    Ok(Server::from_version(COMMIT).unwrap())
}

#[gpui::test]
fn a_new_vs_code_page_opens_its_workspace_folder(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown.panes = serde_json::from_value(serde_json::json!([{
                "pane_id": "p0", "workspace_id": "w0", "tab_id": "t0",
                "label": "shell", "cwd": FOLDER.0, "foreground_cwd": null,
                "focused": true, "right_click_passthrough": false
            }]))
            .unwrap();
            view.live.snapshot = Some(Arc::new(shown));
            view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/?tkn=x").unwrap());
            view.browser.code_server.probe = answers;
        })
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::OpenCode, window, cx)));
    cx.run_until_parked();
    // The server answered; the next tick opens the tab.
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        let tab = cx.global::<Store>().code_tab(&tab_scope, "w0").unwrap();
        assert_eq!(
            tab.location,
            Some(url(&format!(
                "http://127.0.0.1:8000/?tkn=x&folder={}",
                FOLDER.1
            )))
        );
    });
}
