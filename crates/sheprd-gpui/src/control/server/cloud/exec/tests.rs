#![allow(clippy::unwrap_used)]
use super::*;
use crate::{cloud::CloudProvider, cloud::batch::Target};

fn device() -> Device {
    let provider = CloudProvider::ALL[0];
    Device {
        target: Target {
            provider,
            id: "w1".into(),
            endpoint: format!("{}:w1", provider.key()),
            label: "One".into(),
        },
        account: "https://cloud.example.com".into(),
    }
}

fn request(command: &[&str]) -> Request {
    Request {
        device: "w1".into(),
        command: command.iter().map(|word| (*word).to_owned()).collect(),
        cwd: None,
        wait_seconds: 30,
    }
}

fn code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}

#[test]
fn a_command_must_name_a_program_within_bounds() {
    let Err(empty) = start(device(), request(&[])) else {
        panic!("an empty command runs nothing");
    };
    let many = vec!["x"; MAX_WORDS + 1];
    let Err(long) = start(device(), request(&many)) else {
        panic!("a command past the bound runs nothing");
    };
    let expected = if crate::cloud::unavailable().is_some() {
        ErrorCode::Unsupported
    } else {
        ErrorCode::InvalidRequest
    };
    assert_eq!(code(&empty), Some(expected));
    assert_eq!(code(&long), Some(expected));
}

#[test]
fn how_a_command_ended_becomes_its_answer() {
    let finished = exec::Finished {
        code: Some(2),
        stdout: "out".into(),
        stderr: "err".into(),
        truncated: true,
    };
    let Response::Cloud {
        reply: Reply::Exec(info),
    } = respond("coder:w1".into(), Ok(finished))
    else {
        panic!("a finished command answers with its output");
    };
    assert_eq!(
        (
            info.status,
            info.stdout.as_str(),
            info.stderr.as_str(),
            info.truncated
        ),
        (Some(2), "out", "err", true)
    );
    assert_eq!(
        code(&respond("d".into(), Err(Error::Timeout(5)))),
        Some(ErrorCode::Timeout)
    );
    assert_eq!(
        code(&respond("d".into(), Err(Error::Cancelled))),
        Some(ErrorCode::Failed)
    );
}

#[test]
fn dropping_a_running_command_stops_it() {
    let running = Running {
        cancel: Arc::default(),
        answer: Arc::new(Mutex::new(Some(Response::done("x")))),
    };
    let cancel = running.cancel.clone();
    assert!(running.answer().is_some());
    assert!(running.answer().is_none(), "an answer is given once");
    drop(running);
    assert!(cancel.load(Ordering::Acquire));
}
