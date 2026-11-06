//! Shared only by the opt-in integration test harnesses, not production code.
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::{
    ffi::OsStr,
    fs::{self, DirBuilder, File},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static NEXT_SANDBOX: AtomicU64 = AtomicU64::new(0);

/// The shell the isolated daemon starts in each terminal. Windows PowerShell
/// ships with every Windows release, and like `sh` it joins `a"_b"` into
/// `a_b`, so the harnesses' split echo markers work unchanged.
#[cfg(unix)]
const SHELL: &str = "/bin/sh";
#[cfg(windows)]
const SHELL: &str = "powershell.exe";

/// System variables a Windows child cannot start without, copied from the
/// harness. None of them names a user's profile, config, or daemon.
#[cfg(windows)]
const WINDOWS_SYSTEM: [&str; 7] = [
    "COMSPEC",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "PATHEXT",
    "PROCESSOR_ARCHITECTURE",
    "SystemDrive",
    "SystemRoot",
];

pub fn daemon_binary() -> PathBuf {
    let binary = PathBuf::from(
        std::env::var_os("HERDR_TEST_BINARY")
            .expect("set HERDR_TEST_BINARY to an explicit absolute herdr executable"),
    );
    #[cfg(unix)]
    let executable = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o111 != 0;
    #[cfg(windows)]
    let executable = |path: &Path| {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    };
    assert!(
        binary.is_absolute() && binary.is_file() && executable(&binary),
        "explicit daemon binary must be an absolute executable file"
    );
    binary
}

// Owners must stop and reap their children in Drop before this field is dropped.
pub struct Sandbox {
    pub dir: PathBuf,
}

impl Sandbox {
    pub fn new() -> Self {
        let parent = std::env::var_os("HERDR_TEST_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        assert!(parent.is_dir(), "temporary parent must already exist");
        let parent = std::path::absolute(parent).unwrap();
        let dir = parent.join(format!(
            "h{:x}-{:x}-{:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos(),
            NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg(unix)]
        DirBuilder::new().mode(0o700).create(&dir).unwrap();
        // A new directory under the user's temporary directory inherits its
        // owner-only access list.
        #[cfg(windows)]
        DirBuilder::new().create(&dir).unwrap();
        let sandbox = Self { dir };
        // A Windows daemon derives a named pipe from the path instead.
        #[cfg(unix)]
        assert!(
            sandbox.socket().as_os_str().len() < 104,
            "temporary path too long for Unix socket; set HERDR_TEST_TMPDIR to a shorter parent"
        );
        // Herdr asks for a new tab's name by default; the harnesses create
        // tabs by shortcut and expect them at once.
        fs::write(
            sandbox.dir.join("config.toml"),
            format!(
                "onboarding = false\n[ui]\nprompt_new_tab_name = false\n[terminal]\ndefault_shell = \"{SHELL}\"\nshell_mode = \"non_login\"\n"
            ),
        )
        .unwrap();
        sandbox
    }

    pub fn socket(&self) -> PathBuf {
        self.dir.join("a-client.sock")
    }

    pub fn command(&self, binary: impl AsRef<OsStr>, log: impl AsRef<Path>) -> Command {
        let log = File::create(self.dir.join(log)).unwrap();
        let mut command = Command::new(binary);
        command
            .env_clear()
            .env("HOME", &self.dir)
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_CACHE_HOME", self.dir.join("cache"))
            .env("XDG_RUNTIME_DIR", &self.dir)
            .env("TMPDIR", &self.dir)
            .env("HERDR_CONFIG_PATH", self.dir.join("config.toml"))
            .env("HERDR_SOCKET_PATH", self.dir.join("a.sock"))
            .env("HERDR_CLIENT_SOCKET_PATH", self.socket())
            .env("SHELL", SHELL)
            .env("TERM", "xterm-256color")
            .env("PS1", "LIVE> ")
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        #[cfg(unix)]
        command.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        #[cfg(windows)]
        {
            for name in WINDOWS_SYSTEM {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
            let root = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
            let system = root.join("System32");
            command
                .env(
                    "PATH",
                    std::env::join_paths([
                        system.clone(),
                        root,
                        system.join("WindowsPowerShell").join("v1.0"),
                    ])
                    .unwrap(),
                )
                .env("USERPROFILE", &self.dir)
                .env("APPDATA", self.dir.join("appdata"))
                .env("LOCALAPPDATA", self.dir.join("localappdata"))
                .env("TEMP", &self.dir)
                .env("TMP", &self.dir);
        }
        command
    }

    pub fn wait_for_daemon(&self, child: &mut Child, timeout: Duration) {
        eprintln!(
            "isolated daemon pid={} dir={}",
            child.id(),
            self.dir.display()
        );
        let deadline = Instant::now() + timeout;
        while !self.daemon_listening() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "daemon exited during startup"
            );
            assert!(Instant::now() < deadline, "daemon startup timed out");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    fn daemon_listening(&self) -> bool {
        self.socket().exists()
    }

    /// A Windows daemon's named pipe leaves no file behind, so readiness is a
    /// connection that the probe then closes.
    #[cfg(windows)]
    fn daemon_listening(&self) -> bool {
        sheprd_client::Stream::connect(self.socket()).is_ok()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // A child Windows just terminated can hold its log open for a moment
        // after the wait returns; retry briefly instead of leaving the files.
        let deadline = Instant::now() + Duration::from_secs(5);
        while fs::remove_dir_all(&self.dir).is_err()
            && self.dir.exists()
            && cfg!(windows)
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

pub fn stop_children<'a>(children: impl IntoIterator<Item = &'a mut Option<Child>>) {
    // Never use discovery, `server stop`, process-name matching, or group kills.
    for child in children.into_iter().flatten() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn private_unique_directory_is_removed_on_unwind() {
        let first = Sandbox::new();
        let second = Sandbox::new();
        assert_ne!(first.dir, second.dir);
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&first.dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let path = first.dir.clone();
        let result = std::panic::catch_unwind(move || {
            let _sandbox = first;
            panic!("exercise sandbox cleanup");
        });
        assert!(result.is_err());
        assert!(!path.exists());
        assert!(second.dir.exists());
    }

    #[test]
    fn command_has_only_isolated_allowlisted_environment() {
        let sandbox = Sandbox::new();
        #[cfg(unix)]
        let mut command = sandbox.command("/usr/bin/env", "env.log");
        #[cfg(windows)]
        let mut command = {
            let mut command = sandbox.command("cmd.exe", "env.log");
            command.args(["/d", "/c", "set"]);
            command
        };
        let expected = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.unwrap().to_string_lossy().into_owned(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut names = vec![
            "HERDR_CLIENT_SOCKET_PATH",
            "HERDR_CONFIG_PATH",
            "HERDR_SOCKET_PATH",
            "HOME",
            "PATH",
            "PS1",
            "SHELL",
            "TERM",
            "TMPDIR",
            "XDG_CACHE_HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_RUNTIME_DIR",
            "XDG_STATE_HOME",
        ];
        #[cfg(windows)]
        names.extend(
            ["APPDATA", "LOCALAPPDATA", "TEMP", "TMP", "USERPROFILE"]
                .into_iter()
                .chain(
                    WINDOWS_SYSTEM
                        .into_iter()
                        .filter(|name| std::env::var_os(name).is_some()),
                ),
        );
        names.sort_unstable();
        assert_eq!(
            expected.keys().map(String::as_str).collect::<Vec<_>>(),
            names
        );
        assert_eq!(expected["HOME"], sandbox.dir.to_string_lossy());
        assert_eq!(
            expected["HERDR_CLIENT_SOCKET_PATH"],
            sandbox.socket().to_string_lossy()
        );
        assert_eq!(command.get_current_dir(), Some(sandbox.dir.as_path()));
        assert!(command.status().unwrap().success());
        let output = fs::read_to_string(sandbox.dir.join("env.log")).unwrap();
        let actual = output
            .lines()
            .map(|line| {
                let (key, value) = line.split_once('=').unwrap();
                (key.to_owned(), value.to_owned())
            })
            // cmd.exe adds its own PROMPT to what it lists.
            .filter(|(key, _)| !(cfg!(windows) && key == "PROMPT"))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual, expected);
    }
}
