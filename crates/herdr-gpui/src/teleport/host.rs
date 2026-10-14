//! One host Teleport acts on: where its scripts run and which Herdr session
//! its `herdr` CLI addresses.

use super::error::{Error, Result, Step, decode, script};
use herdr_client::{ConnectTarget, ScriptHost, ScriptLimits, run_script, shell_quote};
use serde::de::DeserializeOwned;
use std::{
    io::{Read, Write},
    sync::atomic::AtomicBool,
    time::Duration,
};

/// Output bound for queries; bulk transfers pass their own limits.
const QUERY_OUTPUT: u64 = 32 * 1024 * 1024;
const QUERY_IDLE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Route {
    Local,
    Ssh(herdr_client::SshTarget),
    #[cfg(feature = "cloud")]
    Cloud {
        provider: herdr_client::CloudProvider,
        account: String,
        id: String,
    },
}

/// Largest bundle, session, or archive Teleport will carry.
pub(crate) const TRANSFER_OUTPUT: u64 = 4 * 1024 * 1024 * 1024;
/// Bulk Git work (bundling, fetching) can be quiet for a while.
pub(crate) const TRANSFER_IDLE: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Host {
    route: Route,
    session: Option<String>,
    /// The session belongs to the development installation, which
    /// `herdr --session NAME` cannot address: NAME would resolve to the
    /// release session that shares it. Scripts still run (Git, `bd`), but
    /// `herdr_cli` refuses.
    development: bool,
    /// Tests run "remote" hosts locally in their own environment (home,
    /// daemon socket), exported at the start of every script.
    #[cfg(test)]
    pub(crate) env: Vec<(String, String)>,
}

impl Host {
    /// A host for `target`. Custom sockets are refused: the CLI could not be
    /// pointed at them without also trusting that socket's daemon identity.
    pub(crate) fn new(target: &ConnectTarget) -> Result<Self> {
        match target {
            ConnectTarget::Local => Ok(Self {
                route: Route::Local,
                session: None,
                development: false,
                #[cfg(test)]
                env: Vec::new(),
            }),
            ConnectTarget::Session { name, development } => Ok(Self {
                route: Route::Local,
                session: Some(name.clone()),
                development: *development,
                #[cfg(test)]
                env: Vec::new(),
            }),
            ConnectTarget::Ssh { target, session } => Ok(Self {
                route: Route::Ssh(target.clone()),
                session: Some(session.clone()),
                development: false,
                #[cfg(test)]
                env: Vec::new(),
            }),
            ConnectTarget::Socket(_) | ConnectTarget::Wsl { .. } => Err(Error::UnsupportedHost),
            #[cfg(feature = "cloud")]
            ConnectTarget::Cloud {
                provider,
                account,
                id,
                session,
                ..
            } => Ok(Self {
                route: Route::Cloud {
                    provider: *provider,
                    account: account.clone(),
                    id: id.clone(),
                },
                session: Some(session.clone()),
                development: false,
                #[cfg(test)]
                env: Vec::new(),
            }),
        }
    }

    /// Whether `herdr_cli` can reach this host's session; see `development`.
    pub(crate) fn scripts_herdr(&self) -> bool {
        !self.development
    }

    /// The prelude every script starts with: strict mode, a `PATH` that finds
    /// user-installed tools in a non-interactive SSH shell, and `herdr_cli`
    /// bound to this host's session, or refusing when it cannot be reached.
    fn prelude(&self) -> String {
        let session = self
            .session
            .as_deref()
            .map(|session| format!(" --session {}", shell_quote(session)))
            .unwrap_or_default();
        #[cfg(test)]
        let home = self
            .env
            .iter()
            .map(|(key, value)| format!("{key}={}\nexport {key}\n", shell_quote(value)))
            .collect::<String>();
        #[cfg(not(test))]
        let home = "";
        let herdr_cli = if self.development {
            r#"herdr_cli() { echo "herdr: a development (--dev) session cannot be scripted" >&2; return 97; }"#
                .to_owned()
        } else {
            format!(r#"herdr_cli() {{ herdr{session} "$@"; }}"#)
        };
        format!(
            r#"set -eu
{home}PATH="$HOME/.local/bin:$HOME/.local/share/mise/shims:$HOME/.bun/bin:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/home/linuxbrew/.linuxbrew/bin:$HOME/.nix-profile/bin:$PATH"
export PATH
{herdr_cli}
"#
        )
    }

    /// Run `body` after the prelude, streaming `input` and `output`.
    pub(crate) fn run(
        &self,
        step: Step,
        body: &str,
        input: impl Read + Send,
        output: impl Write + Send,
        limits: ScriptLimits,
        cancelled: &AtomicBool,
    ) -> Result<u64> {
        self.stream(body, input, output, limits, cancelled)
            .map_err(script(step))
    }

    /// [`Host::run`] without Teleport's step context.
    fn stream(
        &self,
        body: &str,
        input: impl Read + Send,
        output: impl Write + Send,
        limits: ScriptLimits,
        cancelled: &AtomicBool,
    ) -> herdr_client::Result<u64> {
        let script_text = format!("{}{body}", self.prelude());
        let host = match &self.route {
            Route::Local => ScriptHost::Local,
            Route::Ssh(target) => ScriptHost::Ssh(target),
            #[cfg(feature = "cloud")]
            Route::Cloud {
                provider,
                account,
                id,
            } => {
                if !cfg!(any(target_os = "linux", target_os = "macos")) {
                    return Err(herdr_client::Error::ScriptUnsupported);
                }
                return crate::cloud::with_command(
                    *provider,
                    account,
                    id,
                    cancelled,
                    |mut command| {
                        command.arg(format!("/bin/sh -c {}", shell_quote(&script_text)));
                        herdr_client::run_script_command(command, input, output, limits, cancelled)
                    },
                )
                .map_err(|error| {
                    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                        herdr_client::Error::ScriptCancelled
                    } else {
                        herdr_client::Error::ScriptConnect(error)
                    }
                })?;
            }
        };
        run_script(host, &script_text, input, output, limits, cancelled)
    }

    /// Run `body` with no input and collect its bounded stdout. Untied to
    /// Teleport's steps, for other features that script a host and keep
    /// their own typed errors.
    pub(crate) fn capture(
        &self,
        body: &str,
        cancelled: &AtomicBool,
    ) -> herdr_client::Result<Vec<u8>> {
        let mut output = Vec::new();
        self.stream(
            body,
            &[][..],
            &mut output,
            ScriptLimits {
                output: QUERY_OUTPUT,
                idle: QUERY_IDLE,
            },
            cancelled,
        )?;
        Ok(output)
    }

    /// The script line running `herdr_cli` with `args`, each quoted.
    pub(crate) fn herdr_line(args: &[&str]) -> String {
        let command = args
            .iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ");
        format!("herdr_cli {command}\n")
    }

    /// Run `body` and collect its bounded stdout.
    pub(crate) fn query(
        &self,
        step: Step,
        body: &str,
        input: &[u8],
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        self.run(
            step,
            body,
            input,
            &mut output,
            ScriptLimits {
                output: QUERY_OUTPUT,
                idle: QUERY_IDLE,
            },
            cancelled,
        )?;
        Ok(output)
    }

    /// Run `herdr_cli` with `args` and decode the result of its envelope.
    pub(crate) fn herdr<T: DeserializeOwned>(
        &self,
        step: Step,
        args: &[&str],
        cancelled: &AtomicBool,
    ) -> Result<T> {
        let output = self.query(step, &Self::herdr_line(args), &[], cancelled)?;
        serde_json::from_slice::<super::snapshot::Envelope<T>>(&output)
            .map(|envelope| envelope.result)
            .map_err(decode(step))
    }

    /// Run `herdr_cli` with `args`, ignoring its output.
    pub(crate) fn herdr_ok(&self, step: Step, args: &[&str], cancelled: &AtomicBool) -> Result<()> {
        let line = Self::herdr_line(args);
        let line = line.trim_end();
        self.query(step, &format!("{line} >/dev/null\n"), &[], cancelled)
            .map(drop)
    }

    /// Which of `programs` the user's login shell can find on this host.
    pub(crate) fn installed(
        &self,
        step: Step,
        programs: &[&str],
        cancelled: &AtomicBool,
    ) -> Result<Vec<String>> {
        self.installed_programs(programs, cancelled)
            .map_err(script(step))
    }

    /// [`Host::installed`] without Teleport's step context.
    pub(crate) fn installed_programs(
        &self,
        programs: &[&str],
        cancelled: &AtomicBool,
    ) -> herdr_client::Result<Vec<String>> {
        if programs.is_empty() {
            return Ok(Vec::new());
        }
        let names = programs
            .iter()
            .map(|p| shell_quote(p))
            .collect::<Vec<_>>()
            .join(" ");
        // The login shell sees version managers configured in the user's
        // profile; the prelude PATH covers the common install locations.
        let body = format!(
            r#"login=$("${{SHELL:-/bin/sh}}" -lc 'printf %s "$PATH"' </dev/null 2>/dev/null || :)
PATH="$PATH:$login"
for program in {names}; do
    if command -v "$program" >/dev/null 2>&1; then printf '%s\n' "$program"; fi
done
"#
        );
        let output = self.capture(&body, cancelled)?;
        Ok(String::from_utf8_lossy(&output)
            .lines()
            .filter(|line| programs.contains(line))
            .map(str::to_owned)
            .collect())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[cfg(feature = "cloud")]
    mod cloud;

    #[test]
    fn hosts_follow_their_endpoint_target() {
        assert_eq!(
            Host::new(&ConnectTarget::Local).unwrap(),
            Host {
                route: Route::Local,
                session: None,
                development: false,
                env: Vec::new(),
            }
        );
        let ssh = Host::new(&ConnectTarget::Ssh {
            target: "me@box".into(),
            session: "work".into(),
        })
        .unwrap();
        assert_eq!(ssh.route, Route::Ssh("me@box".into()));
        assert!(ssh.prelude().contains("herdr --session 'work' \"$@\""));
        assert!(matches!(
            Host::new(&ConnectTarget::Socket("/tmp/s".into())),
            Err(Error::UnsupportedHost)
        ));
        // A `--dev` window's session lives in the development installation,
        // which `herdr --session` cannot name: its host still runs scripts,
        // but Teleport, fan-out, and dispatch are not offered there.
        let development = ConnectTarget::Session {
            name: "default".into(),
            development: true,
        };
        let host = Host::new(&development).unwrap();
        assert!(!host.scripts_herdr());
        assert!(!host.prelude().contains("herdr --session"));
        assert!(matches!(
            crate::teleport::host_for(&development),
            Err(Error::DevelopmentSession)
        ));
        let release = ConnectTarget::Session {
            name: "default".into(),
            development: false,
        };
        assert!(Host::new(&release).unwrap().scripts_herdr());
        assert!(crate::teleport::host_for(&release).is_ok());
    }

    /// Git and `bd` scripts still run on a development host (the orchestrator
    /// reads its repository that way); only `herdr_cli` refuses, so nothing
    /// reaches the release session of the same name.
    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn development_hosts_run_scripts_but_not_herdr() {
        let host = Host::new(&ConnectTarget::Session {
            name: "default".into(),
            development: true,
        })
        .unwrap();
        let cancelled = AtomicBool::new(false);
        assert_eq!(host.capture("printf ok", &cancelled).unwrap(), b"ok");
        let refused = host
            .capture(&Host::herdr_line(&["workspace", "list"]), &cancelled)
            .unwrap_err();
        assert!(
            matches!(
                &refused,
                herdr_client::Error::ScriptExit { status, stderr }
                    if status.code() == Some(97) && stderr.contains("--dev")
            ),
            "{refused:?}"
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn local_scripts_run_under_the_prelude() {
        let host = Host::new(&ConnectTarget::Local).unwrap();
        let cancelled = AtomicBool::new(false);
        let output = host.query(
            Step::Review,
            "printf '%s' \"$1-${PATH%%:*}\" || :\n",
            &[],
            &cancelled,
        );
        // `set -u` makes the unset positional an error: the prelude is active.
        assert!(matches!(
            output,
            Err(Error::Script {
                step: Step::Review,
                ..
            })
        ));
        let output = host
            .query(Step::Review, "cat\n", b"through", &cancelled)
            .unwrap();
        assert_eq!(output, b"through");
        assert_eq!(
            host.installed(
                Step::Review,
                &["sh", "definitely-not-installed-x"],
                &cancelled
            )
            .unwrap(),
            ["sh"]
        );
    }
}
