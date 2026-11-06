use super::*;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
#[path = "tests/linux.rs"]
mod linux;

fn sleep() -> Child {
    Command::new("/bin/sleep").arg("60").spawn().unwrap()
}

#[test]
fn reports_running_as_alive_and_zombie_and_reaped_as_dead() {
    let mut child = sleep();
    let pid = child.id();
    let running = is_dead(pid);

    child.kill().unwrap();
    assert!(!running.unwrap());
    let deadline = Instant::now() + Duration::from_secs(2);
    while !is_dead(pid).unwrap() {
        assert!(Instant::now() < deadline, "killed child did not exit");
        thread::sleep(Duration::from_millis(10));
    }

    child.wait().unwrap();
    assert!(is_dead(pid).unwrap());
}
