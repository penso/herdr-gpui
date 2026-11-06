//! `automation approve` opens the approval dialog for a script the host
//! lists, and nothing more: no approval runs until the user confirms.
use super::*;
use crate::automations::{Dialog, Machine, Read, Wanted};
use crate::control::server::hosts::automation::approve;
use crate::menu::Page;
use crate::teleport::Host;
use sheprd_automation::report::{PROTOCOL, ScriptState, ScriptStatus, Status};
use std::sync::Arc;

const PATH: &str = "/repo/.herdr/automations/nightly.luau";

fn script(path: &str, state: ScriptState) -> ScriptStatus {
    ScriptStatus {
        path: path.into(),
        name: "nightly".into(),
        repo: Some("/repo".into()),
        hash: "0123456789abcdef0123456789abcdef".into(),
        state,
        reason: None,
        caps: None,
        triggers: Vec::new(),
        plan: Some("nightly\n  1. pull main".into()),
        last_run: None,
        running: None,
        tokens_today: None,
    }
}

fn install(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, read: Read) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.automations.install_for_test(
                Wanted {
                    machine: Machine::Local,
                    label: "This Mac".into(),
                    endpoint: Some("local".into()),
                    host: Host::new(&sheprd_client::ConnectTarget::Local).unwrap(),
                },
                read,
            );
        })
    });
}

fn listed() -> Read {
    Read::Ready(Arc::new(Status {
        protocol: PROTOCOL,
        version: "0.1.0".into(),
        scripts: vec![
            script(PATH, ScriptState::AwaitingApproval),
            script("/repo/.herdr/automations/done.luau", ScriptState::Active),
        ],
        ..Status::default()
    }))
}

fn run(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    path: &str,
    host: Option<&str>,
) -> Response {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            approve(view, &caller(Some("w3")), path, host, window, cx)
        })
    })
}

#[gpui::test]
fn a_listed_script_opens_its_approval_and_nothing_runs(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    install(&view, cx, listed());
    let response = run(&view, cx, PATH, None);
    assert!(matches!(response, Response::Done { .. }), "{response:?}");
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::Automation));
        let Some(Dialog::Approve(approve)) = &view.automations.dialog else {
            panic!("no approval dialog");
        };
        assert_eq!(approve.path, PATH);
        assert_eq!(approve.hash, "0123456789abcdef0123456789abcdef");
        assert_eq!(approve.machine, Machine::Local);
        // Asking is all the command does: the host is not busy approving.
        let host = view.automations.host(&Machine::Local).unwrap();
        assert!(host.busy.is_none());
    });
    // A second request while the dialog is open is refused, not stacked.
    let response = run(&view, cx, PATH, Some("local"));
    assert_eq!(error_code(&response), Some(ErrorCode::WindowBusy));
}

#[gpui::test]
fn a_host_is_named_by_endpoint_or_label(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    install(&view, cx, listed());
    let response = run(&view, cx, PATH, Some("This Mac"));
    assert!(matches!(response, Response::Done { .. }), "{response:?}");
}

#[gpui::test]
fn unknown_scripts_hosts_and_approved_scripts_are_refused(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    install(&view, cx, listed());
    for (path, host, code) in [
        (
            "/repo/.herdr/automations/other.luau",
            None,
            ErrorCode::NotFound,
        ),
        (PATH, Some("buildbox"), ErrorCode::NotFound),
        (
            "/repo/.herdr/automations/done.luau",
            None,
            ErrorCode::Failed,
        ),
    ] {
        let response = run(&view, cx, path, host);
        assert_eq!(error_code(&response), Some(code), "{path}: {response:?}");
    }
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert!(view.automations.dialog.is_none());
    });
}

#[gpui::test]
fn a_host_not_read_yet_is_busy(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    install(&view, cx, Read::Unread);
    let response = run(&view, cx, PATH, None);
    assert_eq!(error_code(&response), Some(ErrorCode::Busy));
}
