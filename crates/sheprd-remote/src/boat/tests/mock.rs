//! A scripted Boat API on loopback, exercising the real bounded HTTP path.
#![allow(clippy::unwrap_used)]

use crate::boat::{
    Error, Settings,
    api::{Readiness, wait_ready},
    http,
};
use crate::cloud::tests::server::{Request, serve};
use secrecy::SecretString;
use std::sync::{Arc, Mutex};

fn account(
    handler: impl Fn(&Request) -> (u16, String) + Send + 'static,
) -> (Settings, Arc<Mutex<Vec<Request>>>) {
    let (base, log) = serve(handler);
    let mut settings = super::settings();
    settings.base = format!("{base}/api/v1");
    settings.organization = Some("team_alpha".into());
    settings.machine_type = Some("large".into());
    settings.environment = Some("base".into());
    (settings, log)
}

fn sandbox(state: &str, name: &str) -> String {
    format!(
        r#"{{"id":"bx_23456789","name":"{name}","state":"{state}","desktopAvailable":false,"snapshotAvailable":false}}"#
    )
}

#[test]
fn creating_sends_key_org_options_and_idempotency_then_renames() {
    let (settings, log) =
        account(
            |request| match (request.method.as_str(), request.target.as_str()) {
                ("POST", "/api/v1/sandboxes") => (
                    202,
                    format!(
                        r#"{{"ok":true,"type":"sandbox.created","sandbox":{}}}"#,
                        sandbox("provisioning", "Boat generated")
                    ),
                ),
                ("PATCH", "/api/v1/sandboxes/bx_23456789") => (
                    200,
                    format!(
                        r#"{{"ok":true,"type":"sandbox.info","sandbox":{}}}"#,
                        sandbox("provisioning", "herdr-calm-otter")
                    ),
                ),
                ("GET", "/api/v1/sandboxes?limit=100") => (
                    200,
                    format!(
                        r#"{{"ok":true,"type":"sandbox.list","sandboxes":[{}]}}"#,
                        sandbox("ready", "herdr-calm-otter")
                    ),
                ),
                other => panic!("unexpected {other:?}"),
            },
        );
    let key = SecretString::from("boat_fixture");
    let request = http::Request {
        settings: &settings,
        key: &key,
    };
    let created = request
        .create_for_test("herdr-calm-otter", || false, |_| Ok(()))
        .unwrap();
    assert_eq!(created.name, "herdr-calm-otter");
    assert_eq!(
        request.sandbox_count().unwrap(),
        super::super::api::SandboxCount::Exact(1)
    );
    let log = log.lock().unwrap();
    let create = &log[0];
    assert_eq!(create.authorization.as_deref(), Some("Bearer boat_fixture"));
    let headers = create.headers.to_ascii_lowercase();
    assert!(headers.contains("x-boat-org: team_alpha"));
    assert!(headers.contains("idempotency-key:"));
    let body: serde_json::Value = serde_json::from_str(&create.body).unwrap();
    assert_eq!(body["type"], "large");
    assert_eq!(body["environment"], "base");
    let rename: serde_json::Value = serde_json::from_str(&log[1].body).unwrap();
    assert_eq!(rename["name"], "herdr-calm-otter");
}

#[test]
fn waiting_resumes_an_archived_sandbox_once_and_authorizes_ssh() {
    let polls = Arc::new(Mutex::new(0));
    let counter = polls.clone();
    let (settings, log) = account(move |request| {
        match (request.method.as_str(), request.target.as_str()) {
        ("GET", "/api/v1/sandboxes/bx_23456789") => {
            let mut polls = counter.lock().unwrap();
            *polls += 1;
            let state = match *polls { 1 => "archived", 2 => "provisioning", _ => "ready" };
            (200, format!(r#"{{"ok":true,"type":"sandbox.info","sandbox":{}}}"#, sandbox(state, "herdr-calm-otter")))
        }
        ("POST", "/api/v1/sandboxes/bx_23456789/resume") => (202, format!(r#"{{"ok":true,"type":"sandbox.resuming","sandbox":{}}}"#, sandbox("provisioning", "herdr-calm-otter"))),
        ("POST", "/api/v1/sandboxes/bx_23456789/sshkey") => (200, r#"{"ok":true,"type":"ssh_key.configured","machineIp":"203.0.113.10","sshEndpoint":null,"sshUser":"user","hostKey":"ssh-ed25519 AAAAfixture"}"#.into()),
        other => panic!("unexpected {other:?}"),
    }
    });
    let key = SecretString::from("boat_fixture");
    let request = http::Request {
        settings: &settings,
        key: &key,
    };
    let mut seen = Vec::new();
    let ready = wait_ready(
        &request,
        "bx_23456789",
        || false,
        |state| seen.push(state.clone()),
    )
    .unwrap();
    assert_eq!(ready.name, "herdr-calm-otter");
    assert_eq!(
        seen,
        [
            Readiness::Stopped,
            Readiness::Pending("provisioning".into())
        ]
    );
    let access = request
        .ssh_access("bx_23456789", "ssh-ed25519 AAAApublic")
        .unwrap();
    assert_eq!(access.ssh_user, "user");
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter()
            .filter(|request| request.target.ends_with("/resume"))
            .count(),
        1
    );
    let ssh = log
        .iter()
        .find(|request| request.target.ends_with("/sshkey"))
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(&ssh.body).unwrap();
    assert_eq!(body["key"], "ssh-ed25519 AAAApublic");
}

#[test]
fn missing_sandboxes_and_rejected_keys_keep_their_meaning() {
    let (settings, _) = account(|request| match request.target.as_str() {
        "/api/v1/sandboxes/bx_23456789" => (404, r#"{"message":"Sandbox not found"}"#.into()),
        _ => (401, r#"{"message":"Unauthorized"}"#.into()),
    });
    let key = SecretString::from("boat_fixture");
    let request = http::Request {
        settings: &settings,
        key: &key,
    };
    assert!(matches!(
        request.sandbox("bx_23456789"),
        Err(Error::Deleted)
    ));
    assert!(matches!(
        request.sandbox_count(),
        Err(Error::Authentication)
    ));
    assert!(matches!(
        wait_ready(&request, "bx_23456789", || true, |_| {}),
        Err(Error::Cancelled)
    ));
}
