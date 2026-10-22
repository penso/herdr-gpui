use super::*;
use crate::usage::probe::Shell;
use std::process::{Command, Stdio};

#[test]
fn linuxbrew_is_added_only_for_linux_hosts() {
    for os in ["Darwin", "Linux"] {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        crate::test_executable::write(
            bin.join("uname"),
            format!("#!/bin/sh\nprintf '%s\\n' '{os}'\n"),
            0o755,
        )
        .unwrap();
        let path = std::env::join_paths([
            bin.as_path(),
            std::path::Path::new("/usr/bin"),
            std::path::Path::new("/bin"),
        ])
        .unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .arg("-s")
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut shell = Shell::start(command).unwrap();
        let output = shell
            .run("printf '%s' \"$PATH\"", Duration::from_secs(5))
            .unwrap();
        assert!(output.success);
        let dirs: Vec<_> = output.stdout.split(':').collect();
        assert_eq!(
            dirs.contains(&"/home/linuxbrew/.linuxbrew/bin"),
            os == "Linux",
            "{os}: {}",
            output.stdout
        );
        assert!(output.stdout.ends_with(path.to_str().unwrap()));
    }
}
