use herdr_client::ConnectTarget;
use std::{ffi::OsString, fmt};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaunchMode {
    #[default]
    Normal,
    Help,
    #[cfg(feature = "integration-test")]
    Integration,
    #[cfg(feature = "integration-test")]
    Sidebar,
    #[cfg(feature = "integration-test")]
    Performance,
    #[cfg(feature = "integration-test")]
    Agent,
}

#[derive(Debug)]
pub struct LaunchOptions {
    pub target: ConnectTarget,
    pub mode: LaunchMode,
}

#[derive(Debug)]
pub struct CliError(String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CliError {}

impl LaunchOptions {
    pub fn parse(args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<Self, CliError> {
        let mut args = args.into_iter().map(Into::into);
        let mut socket = None;
        let mut session = None;
        let mut development = false;
        #[cfg(feature = "integration-test")]
        let mut mode = LaunchMode::Normal;
        #[cfg(not(feature = "integration-test"))]
        let mode = LaunchMode::Normal;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--help" | "-h") => {
                    return Ok(Self {
                        target: ConnectTarget::Local,
                        mode: LaunchMode::Help,
                    });
                }
                Some("--socket" | "--session") => {
                    let is_socket = arg == "--socket";
                    let missing = if is_socket {
                        "--socket requires a path"
                    } else {
                        "--session requires a name"
                    };
                    let value = args
                        .next()
                        .filter(|value| {
                            !value.is_empty() && !value.as_encoded_bytes().starts_with(b"-")
                        })
                        .ok_or_else(|| CliError(missing.into()))?;
                    if is_socket {
                        if socket.replace(value).is_some() {
                            return Err(CliError("--socket may only be specified once".into()));
                        }
                    } else {
                        let value = value
                            .into_string()
                            .map_err(|_| CliError("--session requires a UTF-8 name".into()))?;
                        if session.replace(value).is_some() {
                            return Err(CliError("--session may only be specified once".into()));
                        }
                    }
                }
                Some("--dev") => development = true,
                #[cfg(feature = "integration-test")]
                Some(
                    flag @ ("--integration-test" | "--sidebar-test" | "--performance-test"
                    | "--agent-test"),
                ) => {
                    let next = match flag {
                        "--integration-test" => LaunchMode::Integration,
                        "--sidebar-test" => LaunchMode::Sidebar,
                        "--agent-test" => LaunchMode::Agent,
                        _ => LaunchMode::Performance,
                    };
                    if mode != LaunchMode::Normal {
                        return Err(CliError("native test modes are mutually exclusive and may only be specified once".into()));
                    }
                    mode = next;
                }
                _ => {
                    return Err(CliError(format!(
                        "Unknown option: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        if socket.is_some() && (session.is_some() || development) {
            return Err(CliError(
                "--socket cannot be combined with --session or --dev".into(),
            ));
        }
        #[cfg(feature = "integration-test")]
        {
            if matches!(
                mode,
                LaunchMode::Sidebar | LaunchMode::Performance | LaunchMode::Agent
            ) && (socket.is_some() || session.is_some() || development)
            {
                return Err(CliError("fixture tests cannot be combined with connection options or --integration-test".into()));
            }
            if mode == LaunchMode::Integration && socket.is_none() {
                return Err(CliError(
                    "--integration-test requires an explicit --socket".into(),
                ));
            }
            #[cfg(not(target_os = "macos"))]
            if mode == LaunchMode::Performance {
                return Err(CliError(
                    "--performance-test currently requires macOS native event delivery".into(),
                ));
            }
            #[cfg(not(target_os = "macos"))]
            if mode == LaunchMode::Agent {
                return Err(CliError(
                    "--agent-test currently requires macOS native event delivery".into(),
                ));
            }
        }
        let target = match (socket, session) {
            (Some(path), _) => ConnectTarget::Socket(path.into()),
            (_, Some(name)) => ConnectTarget::Session { name, development },
            _ if development => ConnectTarget::Session {
                name: "default".into(),
                development,
            },
            _ => ConnectTarget::Local,
        };
        Ok(Self { target, mode })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn connection_selection() {
        assert!(
            matches!(LaunchOptions::parse(["--dev"]).unwrap().target, ConnectTarget::Session { name, development: true } if name == "default")
        );
        assert!(
            matches!(LaunchOptions::parse(["--session", "test"]).unwrap().target, ConnectTarget::Session { name, development: false } if name == "test")
        );
        assert!(matches!(
            LaunchOptions::parse(std::iter::empty::<OsString>())
                .unwrap()
                .target,
            ConnectTarget::Local
        ));
    }

    #[test]
    fn rejects_missing_and_duplicate_values() {
        for args in [
            vec!["--socket", "--help"],
            vec!["--session", "--dev"],
            vec!["--socket", ""],
            vec!["--socket", "a", "--socket", "b"],
            vec!["--session", "a", "--session", "b"],
        ] {
            assert!(LaunchOptions::parse(args).is_err());
        }
    }

    #[test]
    fn socket_paths_need_not_be_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(b"/tmp/socket-\xff".to_vec());
        let options = LaunchOptions::parse([OsString::from("--socket"), path.clone()]).unwrap();
        assert!(
            matches!(options.target, ConnectTarget::Socket(actual) if actual.as_os_str() == path)
        );
        assert!(LaunchOptions::parse([OsString::from("--session"), path]).is_err());
    }

    #[cfg(feature = "integration-test")]
    #[test]
    fn test_modes_are_exclusive() {
        for first in [
            "--integration-test",
            "--sidebar-test",
            "--performance-test",
            "--agent-test",
        ] {
            for second in [
                "--integration-test",
                "--sidebar-test",
                "--performance-test",
                "--agent-test",
            ] {
                assert!(LaunchOptions::parse([first, second]).is_err());
            }
        }
    }

    #[cfg(all(feature = "integration-test", target_os = "macos"))]
    #[test]
    fn agent_fixture_needs_no_connection() {
        assert_eq!(
            LaunchOptions::parse(["--agent-test"]).unwrap().mode,
            LaunchMode::Agent
        );
        for args in [
            vec!["--agent-test", "--socket", "/unused.sock"],
            vec!["--session", "test", "--agent-test"],
            vec!["--agent-test", "--dev"],
        ] {
            assert!(LaunchOptions::parse(args).is_err());
        }
    }
}
