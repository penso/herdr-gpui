use crate::remote_browser::install::{
    REMOVE_SCRIPT, SHIM, SHIM_COMMAND, SHIM_SCRIPT, SKILL_SCRIPT,
};
use std::{
    io::Write as _,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
};

/// Runs an install script as SSH would, in `home`, with `input` on stdin.
fn run(script: &str, home: &Path, input: &str) {
    let mut child = Command::new("/bin/sh")
        .args(["-c", script])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    // A script that keeps the user's own file exits without reading its
    // input, as run_script allows; the write then races that exit.
    match child.stdin.take().unwrap().write_all(input.as_bytes()) {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
        written => written.unwrap(),
    }
    assert!(child.wait().unwrap().success());
}

#[test]
fn the_shim_is_written_executable_and_a_users_own_file_is_kept() {
    let home = tempfile::tempdir().unwrap();
    let shim = home.path().join(".local/state/herdr/gpui/bin/herdr-gpui");
    run(SHIM_SCRIPT, home.path(), SHIM);
    assert_eq!(std::fs::read_to_string(&shim).unwrap(), SHIM);
    let mode = std::fs::metadata(&shim).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
    // A newer shim replaces the managed one.
    let newer = format!("{SHIM}# newer\n");
    run(SHIM_SCRIPT, home.path(), &newer);
    assert_eq!(std::fs::read_to_string(&shim).unwrap(), newer);
    // No temporary file is left behind.
    assert_eq!(
        std::fs::read_dir(shim.parent().unwrap()).unwrap().count(),
        1
    );

    std::fs::write(&shim, "#!/bin/sh\necho mine\n").unwrap();
    run(SHIM_SCRIPT, home.path(), SHIM);
    assert_eq!(
        std::fs::read_to_string(&shim).unwrap(),
        "#!/bin/sh\necho mine\n"
    );
}

#[test]
fn the_skill_goes_only_into_agent_directories_the_host_has() {
    let home = tempfile::tempdir().unwrap();
    let skill = crate::agent_skill::text_naming(SHIM_COMMAND);
    assert!(skill.contains("~/.local/state/herdr/gpui/bin/herdr-gpui browser open"));
    let file = |root: &str| {
        home.path()
            .join(root)
            .join("skills/herdr-gpui-browser/SKILL.md")
    };

    // No agent directory: nothing is written.
    run(SKILL_SCRIPT, home.path(), &skill);
    assert!(!home.path().join(".claude").exists());

    std::fs::create_dir(home.path().join(".claude")).unwrap();
    std::fs::create_dir_all(file(".agents").parent().unwrap()).unwrap();
    std::fs::write(file(".agents"), "my own skill").unwrap();
    run(SKILL_SCRIPT, home.path(), &skill);
    assert_eq!(std::fs::read_to_string(file(".claude")).unwrap(), skill);
    assert_eq!(
        std::fs::read_to_string(file(".agents")).unwrap(),
        "my own skill"
    );
}

#[test]
fn removing_takes_off_only_what_the_app_wrote() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    std::fs::create_dir(home.path().join(".agents")).unwrap();
    let skill = crate::agent_skill::text_naming(SHIM_COMMAND);
    run(SHIM_SCRIPT, home.path(), SHIM);
    run(SKILL_SCRIPT, home.path(), &skill);
    let own = home
        .path()
        .join(".agents/skills/herdr-gpui-browser/SKILL.md");
    std::fs::write(&own, "my own skill").unwrap();

    run(REMOVE_SCRIPT, home.path(), "");
    assert!(
        !home
            .path()
            .join(".local/state/herdr/gpui/bin/herdr-gpui")
            .exists()
    );
    assert!(
        !home
            .path()
            .join(".claude/skills/herdr-gpui-browser")
            .exists()
    );
    assert_eq!(std::fs::read_to_string(&own).unwrap(), "my own skill");
    // Nothing to remove is not a failure.
    run(REMOVE_SCRIPT, home.path(), "");
}
