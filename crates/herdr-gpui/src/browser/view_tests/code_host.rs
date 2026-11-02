//! A workspace on an SSH host opens VS Code from that host's own server, on
//! the workspace's folder there, and the tabs of this computer's server stay
//! where they are.
use super::*;
use crate::browser::{Scope, code::Reach};
use crate::{
    code_server::{
        Hosts, Server,
        supervisor::{Address, Report, Status},
    },
    config::CodeMode,
    controls::Command,
};
use herdr_client::ConnectTarget;
use std::num::NonZeroU16;

const COMMIT: &str = "2a59476c9bfcb90b3ddc372c36762471b7dfad1c";

fn answers(_: &WebUrl) -> crate::Result<Server> {
    Ok(Server::from_version(COMMIT).unwrap())
}

fn ready(port: u16, token: &str) -> Report {
    Report {
        revision: 1,
        status: Status::Ready,
        address: Some(Address {
            port: NonZeroU16::new(port).unwrap(),
            url: WebUrl::try_from(format!("http://127.0.0.1:{port}/?tkn={token}").as_str())
                .unwrap(),
        }),
    }
}

#[gpui::test]
fn a_remote_workspace_opens_its_folder_on_its_hosts_server(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let local = Scope::local(std::path::Path::new("/tmp/herdr-client.sock"));
    cx.update(|_, cx| {
        Hosts::fixture(cx);
        // A VS Code tab of a workspace on this computer, on its own server.
        Store::update(cx, |store| {
            store.open_editor_tab(
                local.clone(),
                "w9",
                Editor::VsCode,
                Some(url("http://127.0.0.1:8000/?tkn=local")),
            )
        });
        view.update(cx, |view, _| {
            view.endpoints[0].connection.target = ConnectTarget::Ssh {
                target: "dev@build".into(),
                session: "default".into(),
            };
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown.panes = serde_json::from_value(serde_json::json!([{
                "pane_id": "p0", "workspace_id": "w0", "tab_id": "t0",
                "label": "shell", "cwd": "/home/dev/project", "foreground_cwd": null,
                "focused": true, "right_click_passthrough": false
            }]))
            .unwrap();
            view.live.snapshot = Some(Arc::new(shown));
            view.config.code.mode = Some(CodeMode::Start);
            view.config.code.license_accepted = true;
            view.browser.code_server.started_probe = answers;
        })
    });
    // The tab opens before the host's server has an address, which starts it.
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::OpenCode, window, cx)));
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    cx.update(|_, cx| {
        let plan = Hosts::plan(cx, "dev@build").unwrap();
        assert_eq!(plan.host.as_deref(), Some("dev@build"));
        Hosts::set_report(cx, "dev@build", Some(ready(24_000, "host")));
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(matches!(
            view.code_host(cx),
            crate::browser::code_host::CodeHost::Ssh(target) if target == "dev@build"
        ));
        let host_scope = scope(&view.endpoints[0]);
        let store = cx.global::<Store>();
        let tab = store.code_tab(&host_scope, "w0").unwrap();
        assert_eq!(
            tab.location,
            Some(url(
                "http://127.0.0.1:24000/?tkn=host&folder=%2Fhome%2Fdev%2Fproject"
            ))
        );
        // The host's server was asked, and only the window's own server
        // record for the host holds its answer.
        let state = &view.browser.host_code_servers["dev@build"].state;
        assert!(matches!(state, Reach::Ready { .. }), "{state:?}");
        assert_eq!(view.browser.code_server.state, Reach::Unknown);
        // This computer's tab was not moved to the host's server.
        let kept = store.code_tab(&local, "w9").unwrap();
        assert_eq!(kept.location, Some(url("http://127.0.0.1:8000/?tkn=local")));
    });
}

#[gpui::test]
fn a_remote_tab_from_this_computers_server_moves_to_its_host_with_its_folder(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        Hosts::fixture(cx);
        view.update(cx, |view, _| {
            view.endpoints[0].connection.target = ConnectTarget::Ssh {
                target: "dev@build".into(),
                session: "default".into(),
            };
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown.panes = serde_json::from_value(serde_json::json!([{
                "pane_id": "p0", "workspace_id": "w0", "tab_id": "t0",
                "label": "shell", "cwd": "/home/dev/project", "foreground_cwd": null,
                "focused": true, "right_click_passthrough": false
            }]))
            .unwrap();
            view.live.snapshot = Some(Arc::new(shown));
            view.config.code.mode = Some(CodeMode::Start);
            view.config.code.license_accepted = true;
            view.browser.code_server.started_probe = answers;
        });
        // Saved before hosts had servers: on this computer's, with no folder.
        let host_scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            store.open_editor_tab(
                host_scope,
                "w0",
                Editor::VsCode,
                Some(url("http://127.0.0.1:8000/?tkn=local")),
            )
        });
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::OpenCode, window, cx)));
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    cx.update(|_, cx| Hosts::set_report(cx, "dev@build", Some(ready(24_000, "host"))));
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    cx.update(|_, cx| {
        let host_scope = scope(&view.read(cx).endpoints[0]);
        let tab = cx.global::<Store>().code_tab(&host_scope, "w0").unwrap();
        assert_eq!(
            tab.location,
            Some(url(
                "http://127.0.0.1:24000/?folder=%2Fhome%2Fdev%2Fproject"
            ))
        );
    });
}

#[gpui::test]
fn with_only_an_address_a_remote_workspace_uses_it_without_a_folder(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        Hosts::fixture(cx);
        view.update(cx, |view, _| {
            view.endpoints[0].connection.target = ConnectTarget::Ssh {
                target: "dev@build".into(),
                session: "default".into(),
            };
            view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/?tkn=x").unwrap());
            view.browser.code_server.probe = answers;
        })
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::OpenCode, window, cx)));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.code_host(cx),
            crate::browser::code_host::CodeHost::Local
        );
        let tab = cx
            .global::<Store>()
            .code_tab(&scope(&view.endpoints[0]), "w0")
            .unwrap();
        assert_eq!(tab.location, Some(url("http://127.0.0.1:8000/?tkn=x")));
    });
}

/// Every remote tab that moved from this computer's server gets its own
/// workspace's folder, not only the one focused as the host's server came.
#[gpui::test]
fn each_moved_remote_tab_gets_its_own_folder(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        Hosts::fixture(cx);
        view.update(cx, |view, _| {
            view.endpoints[0].connection.target = ConnectTarget::Ssh {
                target: "dev@build".into(),
                session: "default".into(),
            };
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            let mut other = shown.tabs[0].clone();
            other.tab_id = "t9".into();
            other.workspace_id = "w1".into();
            shown.tabs.push(other);
            shown.panes = serde_json::from_value(serde_json::json!([
                { "pane_id": "p0", "workspace_id": "w0", "tab_id": "t0",
                  "label": "shell", "cwd": "/home/dev/one", "foreground_cwd": null,
                  "focused": true, "right_click_passthrough": false },
                { "pane_id": "p9", "workspace_id": "w1", "tab_id": "t9",
                  "label": "shell", "cwd": "/home/dev/two", "foreground_cwd": null,
                  "focused": false, "right_click_passthrough": false }
            ]))
            .unwrap();
            view.live.snapshot = Some(Arc::new(shown));
            view.config.code.mode = Some(CodeMode::Start);
            view.config.code.license_accepted = true;
            view.browser.code_server.started_probe = answers;
        });
        // Both saved before hosts had servers: on this computer's, no folder.
        let host_scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            for workspace in ["w0", "w1"] {
                store.open_editor_tab(
                    host_scope.clone(),
                    workspace,
                    Editor::VsCode,
                    Some(url("http://127.0.0.1:8000/?tkn=local")),
                );
            }
        });
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::OpenCode, window, cx)));
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    cx.update(|_, cx| Hosts::set_report(cx, "dev@build", Some(ready(24_000, "host"))));
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    // The other workspace is focused only after its tab moved.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown.focused_workspace_id = Some("w1".into());
            view.live.snapshot = Some(Arc::new(shown));
            view.ensure_code_page(window, cx);
        })
    });
    cx.update(|_, cx| {
        let host_scope = scope(&view.read(cx).endpoints[0]);
        let store = cx.global::<Store>();
        let folder = |workspace| {
            store
                .code_tab(&host_scope, workspace)
                .unwrap()
                .location
                .clone()
        };
        assert_eq!(
            folder("w0"),
            Some(url("http://127.0.0.1:24000/?folder=%2Fhome%2Fdev%2Fone"))
        );
        assert_eq!(
            folder("w1"),
            Some(url("http://127.0.0.1:24000/?folder=%2Fhome%2Fdev%2Ftwo"))
        );
    });
}

/// A host's folder is a POSIX path on the host, checked as one whatever
/// this computer's paths look like: Windows calls `/home/dev` relative.
#[gpui::test]
fn only_an_absolute_posix_root_is_opened_on_a_host(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let root = |cwd: &'static str| {
        move |view: &mut HerdrWindow| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown.panes = serde_json::from_value(serde_json::json!([{
                "pane_id": "p0", "workspace_id": "w0", "tab_id": "t0",
                "label": "shell", "cwd": cwd, "foreground_cwd": null,
                "focused": true, "right_click_passthrough": false
            }]))
            .unwrap();
            view.live.snapshot = Some(Arc::new(shown));
            view.host_editor_root().map(str::to_owned)
        }
    };
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert_eq!(
                root("/home/dev/project")(view).as_deref(),
                Some("/home/dev/project")
            );
            assert_eq!(root("project")(view), None);
            assert_eq!(root(r"C:\Users\dev")(view), None);
        })
    });
}
