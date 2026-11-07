//! The environment of the SSH tools this crate starts. A GUI application on
//! macOS inherits launchd's environment: a `PATH` without Homebrew and the
//! system agent socket, so `ssh` would be the system build and would not see
//! the keys a user's shell configures, such as a security key in another
//! agent. The application supplies how its children get the user's login
//! environment, and every `ssh` and `ssh-keygen` started here uses it.
//!
//! On Windows the tools are OpenSSH for Windows: the copy Windows installs as
//! an optional feature, and only without it the first one on `PATH`.
#[cfg(windows)]
use std::path::{Path, PathBuf};
use std::{process::Command, sync::OnceLock};

static APPLY: OnceLock<fn(&mut Command)> = OnceLock::new();

/// Sets, once per process, how this crate's SSH children get the user's
/// environment. `apply` may block the first time it runs; it is only called
/// where the command is built, which is off the UI thread. Returns `false`
/// when one was already set, which is kept.
pub fn set_command_environment(apply: fn(&mut Command)) -> bool {
    APPLY.set(apply).is_ok()
}

/// `program`, an OpenSSH tool such as `ssh`, with the application's
/// environment applied. On Unix, a `PATH` set on the command is also where
/// `program` is looked up; on Windows it is found by [`locate`].
pub(crate) fn command(program: &str) -> Command {
    #[cfg(windows)]
    let mut command = crate::new_command(locate(
        program,
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("SystemRoot").as_deref(),
        Path::is_file,
    ));
    #[cfg(not(windows))]
    let mut command = crate::new_command(program);
    if let Some(apply) = APPLY.get() {
        apply(&mut command);
    }
    command
}

/// Where `program` is on Windows: `program.exe` in `SystemRoot\System32\OpenSSH`,
/// else in the first absolute `PATH` directory that has it. The system copy
/// comes first because `PATH` often leads with Git's MSYS `ssh`, which keeps
/// its own home and prints known hosts files as `/c/...` paths, so neither the
/// user's config nor their host keys would be the ones Windows OpenSSH uses.
/// A relative `PATH` entry would resolve against whatever directory this
/// process has, so it is skipped. With neither, the bare name is spawned and fails as not
/// found, which callers already report.
#[cfg(windows)]
fn locate(
    program: &str,
    path: Option<&std::ffi::OsStr>,
    system_root: Option<&std::ffi::OsStr>,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    let file = format!("{program}.exe");
    system_root
        .map(|root| Path::new(root).join("System32").join("OpenSSH"))
        .into_iter()
        .chain(
            path.into_iter()
                .flat_map(std::env::split_paths)
                .filter(|dir| dir.is_absolute()),
        )
        .map(|dir| dir.join(&file))
        .find(|candidate| exists(candidate))
        .unwrap_or_else(|| PathBuf::from(file))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Test fixtures only.
mod tests {
    use super::*;
    use std::ffi::OsStr;

    // The only test in this binary that sets the process-wide hook.
    #[test]
    fn ssh_children_get_the_application_environment() {
        fn apply(command: &mut Command) {
            command.env("HERDR_TEST_LOGIN_ENVIRONMENT", "1");
        }
        assert!(set_command_environment(apply));
        assert!(!set_command_environment(|_| {}));
        let command = command("ssh");
        #[cfg(unix)]
        assert_eq!(command.get_program(), "ssh");
        #[cfg(windows)]
        assert!(Path::new(command.get_program()).ends_with("ssh.exe"));
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "HERDR_TEST_LOGIN_ENVIRONMENT"
                    && value == Some(OsStr::new("1")))
        );
    }

    #[cfg(windows)]
    #[test]
    fn openssh_is_the_system_copy_then_the_first_absolute_path_entry() {
        let path = std::env::join_paths([
            r"relative\bin",
            r"C:\Program Files\Git\usr\bin",
            r"C:\OpenSSH",
        ])
        .unwrap();
        let root = OsStr::new(r"C:\Windows");
        let at = |found: &'static [&'static str]| {
            move |candidate: &Path| found.iter().any(|file| candidate == Path::new(file))
        };
        // Git's MSYS `ssh` leading PATH never shadows the system copy.
        assert_eq!(
            locate(
                "ssh",
                Some(&path),
                Some(root),
                at(&[
                    r"C:\Program Files\Git\usr\bin\ssh.exe",
                    r"C:\Windows\System32\OpenSSH\ssh.exe"
                ])
            ),
            Path::new(r"C:\Windows\System32\OpenSSH\ssh.exe")
        );
        // Without the optional feature, PATH decides, in order, and a
        // relative entry is never searched even when it has the tool.
        assert_eq!(
            locate(
                "ssh-keygen",
                Some(&path),
                Some(root),
                at(&[r"relative\bin\ssh-keygen.exe", r"C:\OpenSSH\ssh-keygen.exe"])
            ),
            Path::new(r"C:\OpenSSH\ssh-keygen.exe")
        );
        assert_eq!(
            locate(
                "ssh",
                Some(&path),
                None,
                at(&[r"C:\Windows\System32\OpenSSH\ssh.exe"])
            ),
            Path::new("ssh.exe")
        );
        assert_eq!(
            locate("ssh", Some(&path), Some(root), at(&[])),
            Path::new("ssh.exe")
        );
    }
}
