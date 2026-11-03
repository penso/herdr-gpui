use super::*;
use sheprd_client::{ScriptLimits, run_script_command};
use std::{io, sync::atomic::AtomicBool, time::Duration};

#[test]
fn scripts_keep_the_pinned_host_key_until_completion_and_remove_it_on_every_exit() {
    let dir = tempfile::tempdir().unwrap();
    let private = dir.path().join("key");
    for cancelled in [false, true] {
        let gateway = Gateway::new(&access(), &private).unwrap();
        let known_hosts = gateway._known_hosts.path().to_owned();
        let result = gateway.with_command(|_| {
            assert!(known_hosts.is_file());
            let mut child = Command::new("/bin/sh");
            child
                .args(["-c", "test -s \"$1\" || exit 90; cat; exit 7", "sh"])
                .arg(&known_hosts);
            run_script_command(
                child,
                &b"\0binary\xff"[..],
                io::sink(),
                ScriptLimits {
                    output: 1024,
                    idle: Duration::from_secs(5),
                },
                &AtomicBool::new(cancelled),
            )
        });
        if cancelled {
            assert!(matches!(result, Err(sheprd_client::Error::ScriptCancelled)));
        } else {
            assert!(matches!(result,
                Err(sheprd_client::Error::ScriptExit { status, .. }) if status.code() == Some(7)));
        }
        assert!(!known_hosts.exists());
    }
}
