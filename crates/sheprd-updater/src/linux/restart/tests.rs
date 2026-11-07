use super::*;
use std::{os::unix::net::UnixListener, process::Child};

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn old_process() -> Result<OwnedChild> {
    Ok(OwnedChild(
        Command::new("/bin/sh")
            .args(["-c", "read -r command"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    ))
}

#[test]
fn replacement_waits_for_old_process_and_can_bind_its_control_socket() -> anyhow::Result<()> {
    let stage = tempfile::tempdir()?;
    let socket = stage.path().join("control.sock");
    let old_socket = UnixListener::bind(&socket)?;
    let mut old = old_process()?;
    let pid = Pid::from_raw(old.0.id().try_into()?).ok_or(Error::MalformedRequest)?;
    let pidfd = pidfd_open(pid, PidfdFlags::empty())?;
    assert!(matches!(
        wait_for_exit(&pidfd, Duration::ZERO),
        Err(Error::HelperTimeout)
    ));
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let replacement = socket.clone();
    let waiter = thread::spawn(move || {
        let _ = ready_tx.send(());
        let result = after_commit(COMMIT.as_slice(), &pidfd, || {
            // A pre-exit launch would collide with the old control endpoint.
            UnixListener::bind(replacement)?;
            Ok(())
        });
        let _ = done_tx.send(result);
    });
    ready_rx.recv_timeout(Duration::from_secs(5))?;
    assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(old.0.try_wait()?.is_none());
    // Simulate the old application's service teardown, then allow its exit.
    drop(old_socket);
    fs::remove_file(&socket)?;
    old.0
        .stdin
        .take()
        .ok_or(Error::MissingControlPipe)?
        .write_all(b"exit\n")?;
    old.0.wait()?;
    done_rx.recv_timeout(Duration::from_secs(5))??;
    assert!(
        socket.exists(),
        "the replacement acquired the control endpoint"
    );
    waiter
        .join()
        .map_err(|_| anyhow::anyhow!("relaunch waiter panicked"))?;
    Ok(())
}

#[test]
fn dropping_or_corrupting_a_restart_request_never_launches() -> Result<()> {
    let parent =
        pidfd_open(rustix::process::getpid(), PidfdFlags::empty()).map_err(io::Error::from)?;
    for instruction in [b"".as_slice(), b"COM", b"CANCEL\n"] {
        assert!(matches!(
            after_commit(instruction, &parent, || panic!("uncommitted restart")),
            Err(Error::NotCommitted)
        ));
    }
    Ok(())
}

#[test]
fn relaunch_arguments_are_checked_before_gui_startup() {
    assert_eq!(run_helper(&[FLAG.into()]), Some(ExitCode::FAILURE));
    assert_eq!(
        run_helper(&[FLAG.into(), "not-a-pid".into()]),
        Some(ExitCode::FAILURE)
    );
    assert_eq!(run_helper(&["--help".into()]), None);
}
