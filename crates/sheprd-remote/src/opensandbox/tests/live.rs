//! Opt-in: drives a real OpenSandbox server named by
//! `HERDR_TEST_OPENSANDBOX_URL` with `HERDR_TEST_OPENSANDBOX_KEY`: makes a
//! sandbox, sends binary input with an end of file over execd, installs Herdr
//! from the network, waits for the entrypoint's server, and deletes it. Run
//! with `cargo test -p sheprd-remote --features opensandbox opensandbox::tests::live -- --ignored`.

use super::*;
use crate::opensandbox::{
    api::Api,
    relay::{attach, relay, start},
};
use std::io::Cursor;

/// Run `remote` in the sandbox with `input`, as the relay would.
fn exec(api: &Api, id: &str, remote: &str, input: Vec<u8>) -> (u8, Vec<u8>) {
    let execd = api.execd(id).unwrap();
    let session = start(&execd, remote).unwrap();
    let mut socket = attach(&execd, &session).unwrap();
    let mut stdout = Vec::new();
    let code = relay(
        &mut socket,
        crate::cloud::socket::input(Cursor::new(input)),
        &mut stdout,
        &mut Vec::new(),
    )
    .unwrap();
    (code, stdout)
}

#[test]
#[ignore = "needs a running OpenSandbox server and network access"]
fn a_sandbox_is_made_reached_and_deleted() {
    let url = std::env::var("HERDR_TEST_OPENSANDBOX_URL")
        .unwrap_or_else(|_| panic!("set HERDR_TEST_OPENSANDBOX_URL"));
    let key: secrecy::SecretString = std::env::var("HERDR_TEST_OPENSANDBOX_KEY")
        .unwrap_or_else(|_| panic!("set HERDR_TEST_OPENSANDBOX_KEY"))
        .into();
    let settings = settings(&url);
    let api = Api {
        settings: &settings,
        key: &key,
    };
    let never = || false;
    let sandbox = api.create(&setup::random_name()).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        api.wait_running(&sandbox.id, &never).unwrap();
        // Every byte value, and an end of file the command waits for.
        let payload: Vec<u8> = (0..=255u8).cycle().take(300_000).collect();
        let (code, stdout) = exec(&api, &sandbox.id, "wc -c; exit 7", payload);
        assert_eq!(
            (code, String::from_utf8_lossy(&stdout).trim()),
            (7, "300000")
        );
        let (code, stdout) = exec(
            &api,
            &sandbox.id,
            "curl -fsSL https://herdr.dev/install.sh | sh >/dev/null 2>&1; $HOME/.local/bin/herdr --version",
            Vec::new(),
        );
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stdout));
        // The entrypoint starts the server once Herdr is there.
        let up = (0..30).any(|_| {
            std::thread::sleep(std::time::Duration::from_secs(1));
            exec(
                &api,
                &sandbox.id,
                "$HOME/.local/bin/herdr workspace list",
                Vec::new(),
            )
            .1
            .windows(14)
            .any(|window| window == b"workspace_list")
        });
        assert!(up, "Herdr's server never answered");
        assert!(
            api.ours()
                .unwrap()
                .iter()
                .any(|found| found.id == sandbox.id)
        );
    }));
    api.delete_sandbox(&sandbox.id).unwrap();
    assert!(matches!(
        api.sandbox(&sandbox.id),
        Err(Error::Gone(_)) | Ok(_)
    ));
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
