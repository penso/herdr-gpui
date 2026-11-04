use super::*;
use sheprd_client::{ScriptLimits, run_script_command};
use std::{io, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

#[test]
fn teleport_keeps_gateway_credentials_until_the_command_finishes_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    for cancelled in [false, true] {
        let gateway = Gateway::new(
            &access("ssh fixture-token@gw.example.com", "fixture-token"),
            &dir.path().join("known_hosts"),
        )
        .unwrap();
        let config = PathBuf::from(gateway.command.get_args().nth(1).unwrap());
        let result = gateway.with_command(|command| {
            assert!(config.is_file());
            // Stand in for SSH: consume the exact private config path its
            // command would read, then stream data through a real child.
            let mut child = Command::new("/bin/sh");
            child
                .args(["-c", "test -s \"$1\" || exit 90; cat; exit 7", "sh"])
                .arg(command.get_args().nth(1).unwrap());
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
            assert!(
                matches!(result, Err(sheprd_client::Error::ScriptExit { status, .. }) if status.code() == Some(7))
            );
        }
        assert!(!config.exists(), "private token file removed on every exit");
    }
}
