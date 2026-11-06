//! Opt-in: makes a real boxd machine with the key in `HERDR_BOXD_API_KEY`,
//! runs commands through the relay's exec stream (binary stdin with its end,
//! stderr, exit codes), puts the machine to sleep and back each way boxd has,
//! and deletes it again. Herdr's installer and bridge need this app's own
//! executable as the relay, so they are exercised with a built `sheprd`. Extra labels for
//! the machine, such as a team's, come from `HERDR_TEST_BOXD_LABELS`
//! (`key=value,key=value`). Run with
//! `cargo test -p sheprd-remote --all-features boxd::tests::live -- --ignored --nocapture`.

use super::super::{
    api::Readiness,
    http::Request,
    relay::{open, relay},
    setup, store,
    wire::Message,
};
use super::*;
use crate::cloud::socket::input;
use std::io::Cursor;

fn live_settings() -> Settings {
    let mut settings =
        settings(&std::env::var("HERDR_BOXD_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.into()));
    if let Ok(labels) = std::env::var("HERDR_TEST_BOXD_LABELS") {
        for pair in labels.split(',').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap();
            settings.labels.insert(key.into(), value.into());
        }
    }
    settings.labels.insert("sheprd-test".into(), "live".into());
    settings
}

/// Run `command` on `id` through the relay, in this process.
fn run(settings: &Settings, id: &str, command: &str, stdin: &[u8]) -> (u8, Vec<u8>, Vec<u8>) {
    let key = store::key(settings).unwrap();
    let mut connection = open(settings, &key, id, command).unwrap();
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = relay(
        &mut connection,
        input(Cursor::new(stdin.to_vec())),
        &mut stdout,
        &mut stderr,
    )
    .unwrap();
    (code, stdout, stderr)
}

fn sleep_and_return(request: &Request, id: &str, method: &str, expect: Readiness) {
    let message = Message::default().string(1, id).into_bytes();
    request.call("test", method, &message).unwrap();
    // The call returns before the status settles.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let status = request.machine(id).unwrap().status;
        if Readiness::from(status.as_str()) == expect {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{method} left it {status}"
        );
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let seen = std::sync::Mutex::new(Vec::new());
    let ready = request
        .wait_ready(
            id,
            || false,
            |readiness| seen.lock().unwrap().push(readiness),
        )
        .unwrap();
    assert_eq!(ready.status, "running");
    assert_eq!(
        seen.lock().unwrap().first(),
        Some(&expect),
        "after {method}"
    );
    eprintln!("{method}: back with {:?}", seen.lock().unwrap());
}

#[test]
#[ignore = "needs a boxd API key in HERDR_BOXD_API_KEY and network access"]
fn a_machine_is_made_reached_slept_woken_and_deleted() {
    let settings = live_settings();
    let key = store::key(&settings).unwrap();
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let name = std::env::var("HERDR_TEST_BOXD_NAME").unwrap_or_else(|_| setup::random_name());
    let machine = request.create(&name).unwrap();
    eprintln!(
        "created {} ({}) {}",
        machine.name, machine.id, machine.status
    );
    let id = machine.id.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        request.wait_ready(&id, || false, |_| {}).unwrap();
        assert!(request.machines().unwrap().iter().any(|m| m.id == id));

        let bytes: Vec<u8> = (0..=255u8).cycle().take(1 << 20).collect();
        let (code, stdout, _) = run(&settings, &id, "cat", &bytes);
        assert_eq!(code, 0);
        assert_eq!(stdout.len(), bytes.len());
        assert!(
            stdout == bytes,
            "a MiB of every byte value comes back unchanged"
        );
        let (code, stdout, _) = run(&settings, &id, "wc -c", &bytes);
        assert_eq!(
            (code, String::from_utf8_lossy(&stdout).trim()),
            (0, "1048576")
        );
        let (code, stdout, stderr) = run(&settings, &id, "echo out; echo err >&2; exit 3", b"");
        assert_eq!(
            (code, &stdout[..], &stderr[..]),
            (3, &b"out\n"[..], &b"err\n"[..])
        );
        let (code, stdout, _) = run(&settings, &id, "/bin/sh -c 'whoami; echo $HOME'", b"");
        eprintln!(
            "runs as {}",
            String::from_utf8_lossy(&stdout).replace('\n', " ")
        );
        assert_eq!(code, 0);

        sleep_and_return(&request, &id, "SuspendVm", Readiness::Suspended);
        sleep_and_return(&request, &id, "HibernateVm", Readiness::Hibernated);
        sleep_and_return(&request, &id, "StopVm", Readiness::Stopped);
        let (code, stdout, _) = run(&settings, &id, "echo back", b"");
        assert_eq!((code, &stdout[..]), (0, &b"back\n"[..]));
    }));
    request.delete(&id).unwrap();
    assert!(matches!(request.machine(&id), Err(Error::Gone(_))));
    eprintln!("deleted {id}");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[test]
#[ignore = "needs network access"]
fn a_wrong_key_is_refused_as_such() {
    let settings = live_settings();
    let key = secrecy::SecretString::from("not-a-real-key");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let error = request.machines().unwrap_err();
    eprintln!("wrong key: {error}");
    assert!(matches!(error, Error::Authentication), "{error:?}");
    let error = open(&settings, &key, "anything", "true")
        .and_then(|mut connection| {
            relay(
                &mut connection,
                input(Cursor::new(Vec::new())),
                &mut Vec::new(),
                &mut Vec::new(),
            )
        })
        .unwrap_err();
    eprintln!("wrong key on exec: {error}");
    assert!(matches!(error, Error::Authentication), "{error:?}");
}
