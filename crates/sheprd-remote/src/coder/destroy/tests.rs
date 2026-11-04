#![allow(clippy::unwrap_used)]
use super::*;
use std::sync::{Arc, Mutex};

type Request = crate::cloud::tests::server::Request;

fn serve(
    handler: impl Fn(&Request) -> (u16, String) + Send + 'static,
) -> (Settings, Arc<Mutex<Vec<Request>>>) {
    let (base, log) = crate::cloud::tests::server::serve(handler);
    let mut settings = crate::coder::tests::settings();
    settings.base = base;
    (settings, log)
}

fn workspace(transition: &str, status: &str, job: &str) -> String {
    format!(
        r#"{{"id":"w1","name":"herdr-box","latest_build":{{"transition":"{transition}",
            "status":"{status}","job":{{"status":"{job}","error":"no\nprovisioner"}},"resources":[]}}}}"#
    )
}

fn tokens(_: bool) -> Result<SecretString> {
    Ok("fresh".into())
}

/// Answer each poll of `w1` with the next reply, repeating the last one.
fn scripted(replies: Vec<(u16, String)>) -> (Settings, Arc<Mutex<Vec<Request>>>) {
    let polls = Mutex::new(0);
    serve(
        move |request| match (request.method.as_str(), request.target.as_str()) {
            ("GET", "/api/v2/workspaces/w1") => {
                let mut polls = polls.lock().unwrap();
                let reply = replies[(*polls).min(replies.len() - 1)].clone();
                *polls += 1;
                reply
            }
            ("POST", "/api/v2/workspaces/w1/builds") => {
                assert_eq!(request.body, r#"{"transition":"delete"}"#);
                (201, r#"{"id":"b9"}"#.into())
            }
            other => panic!("unexpected {other:?}"),
        },
    )
}

fn posts(log: &Mutex<Vec<Request>>) -> usize {
    log.lock()
        .unwrap()
        .iter()
        .filter(|request| request.method == "POST")
        .count()
}

#[test]
fn an_idle_workspace_gets_one_delete_build_and_reports_each_stage_until_gone() {
    let (settings, log) = scripted(vec![
        (200, workspace("start", "running", "succeeded")),
        (200, workspace("delete", "pending", "pending")),
        (200, workspace("delete", "deleting", "running")),
        (410, r#"{"message":"Workspace was deleted."}"#.into()),
    ]);
    let mut stages = Vec::new();
    destroy(
        &settings,
        &tokens,
        "w1",
        || false,
        |stage| stages.push(stage),
    )
    .unwrap();
    assert_eq!(stages, [Stage::Queued, Stage::Queued, Stage::Deleting]);
    assert_eq!(posts(&log), 1, "delete is requested exactly once");
    assert!(
        log.lock()
            .unwrap()
            .iter()
            .all(|request| request.authorization.as_deref() == Some("Bearer fresh"))
    );
}

#[test]
fn a_running_build_is_waited_out_before_deleting() {
    let (settings, log) = scripted(vec![
        (200, workspace("start", "starting", "running")),
        (200, workspace("start", "running", "succeeded")),
        (200, workspace("delete", "deleted", "succeeded")),
    ]);
    let mut stages = Vec::new();
    destroy(
        &settings,
        &tokens,
        "w1",
        || false,
        |stage| stages.push(stage),
    )
    .unwrap();
    assert_eq!(stages, [Stage::Waiting, Stage::Queued]);
    assert_eq!(posts(&log), 1);
}

#[test]
fn a_workspace_already_deleting_or_gone_is_not_deleted_again() {
    let (settings, log) = scripted(vec![
        (200, workspace("delete", "deleting", "running")),
        (200, workspace("delete", "deleted", "succeeded")),
    ]);
    destroy(&settings, &tokens, "w1", || false, |_| {}).unwrap();
    assert_eq!(posts(&log), 0);

    let (settings, log) = scripted(vec![(404, r#"{"message":"Resource not found"}"#.into())]);
    destroy(&settings, &tokens, "w1", || false, |_| {}).unwrap();
    assert_eq!(posts(&log), 0);
}

#[test]
fn a_failed_delete_build_keeps_coders_reason_and_cancelling_stops_polling() {
    let (settings, log) = scripted(vec![
        (200, workspace("start", "running", "succeeded")),
        (200, workspace("delete", "failed", "failed")),
    ]);
    let error = destroy(&settings, &tokens, "w1", || false, |_| {}).unwrap_err();
    assert!(
        matches!(&error, Error::Workspace(reason) if reason == "no provisioner"),
        "{error:?}"
    );
    assert_eq!(
        posts(&log),
        1,
        "the attempt's own failed delete is not retried"
    );

    let (settings, log) = scripted(vec![(200, workspace("start", "running", "succeeded"))]);
    assert!(matches!(
        destroy(&settings, &tokens, "w1", || true, |_| {}),
        Err(Error::Cancelled)
    ));
    assert!(
        log.lock().unwrap().is_empty(),
        "nothing is sent once cancelled"
    );
}

#[test]
fn a_delete_that_failed_or_was_canceled_before_is_queued_again() {
    for status in ["failed", "canceled"] {
        let (settings, log) = scripted(vec![
            (200, workspace("delete", status, status)),
            (200, workspace("delete", "deleting", "running")),
            (410, r#"{"message":"Workspace was deleted."}"#.into()),
        ]);
        let mut stages = Vec::new();
        destroy(
            &settings,
            &tokens,
            "w1",
            || false,
            |stage| stages.push(stage),
        )
        .unwrap();
        assert_eq!(stages, [Stage::Queued, Stage::Deleting], "{status}");
        assert_eq!(posts(&log), 1, "{status}");
    }
}

#[test]
fn a_refused_delete_request_is_reported_and_nothing_is_retried() {
    let (settings, log) = serve(|request| match request.method.as_str() {
        "GET" => (200, workspace("start", "running", "succeeded")),
        _ => (403, r#"{"message":"Insufficient permissions."}"#.into()),
    });
    let error = destroy(&settings, &tokens, "w1", || false, |_| {}).unwrap_err();
    assert!(matches!(error, Error::Forbidden), "{error:?}");
    assert_eq!(posts(&log), 1);
}
