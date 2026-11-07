use crossbeam_channel::bounded;

use super::*;

#[test]
fn a_replaced_process_says_nothing_of_its_exit() {
    let (inputs, received) = bounded(16);
    let launcher = Launcher::new("sh".into(), Vec::new(), Vec::new(), Path::new("/"), inputs);
    let args = vec!["-c".into(), "sleep 30".into()];
    launcher.start(args, Box::new(crate::harness::Acp)).unwrap();
    launcher.stop();
    let quiet = received
        .recv_timeout(std::time::Duration::from_millis(300))
        .ok();
    assert!(
        !matches!(quiet, Some(Input::AgentExited { .. })),
        "the stopped process was reported as the chat's agent exiting"
    );
}

#[test]
fn the_running_process_still_reports_its_exit() {
    let (inputs, received) = bounded(16);
    let launcher = Launcher::new("sh".into(), Vec::new(), Vec::new(), Path::new("/"), inputs);
    let args = vec!["-c".into(), "exit 3".into()];
    launcher.start(args, Box::new(crate::harness::Acp)).unwrap();
    let deadline = std::time::Duration::from_secs(5);
    loop {
        match received.recv_timeout(deadline).unwrap() {
            Input::AgentExited { code, .. } => break assert_eq!(code, Some(3)),
            _ => continue,
        }
    }
}
