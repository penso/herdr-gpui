//! Command-line parsing, kept pure over OS strings so it is testable and
//! exits before any socket is bound.

use serde_json::{Value, json};
use std::{
    ffi::OsString,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

pub const TOKEN_ENV: &str = "HERDR_COMPANION_TOKEN";
const DEFAULT_LISTEN: &str = "127.0.0.1:8787";
const DEFAULT_DECISION_SECS: u64 = 110;
/// Headroom between the companion giving up and Claude Code giving up, so the
/// companion always answers first and the terminal prompt appears cleanly.
const HOOK_HEADROOM_SECS: u64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeOptions {
    pub listen: SocketAddr,
    pub decision_timeout: Duration,
    pub herdr_socket: Option<PathBuf>,
    /// An ntfy topic URL that receives push notices.
    pub ntfy: Option<String>,
    /// Include commands and messages in push notices, not only their kind.
    pub ntfy_details: bool,
    /// Where the phone reaches the web app; tapped notices and the pairing
    /// QR code open it.
    pub public_url: Option<String>,
    /// Skip the pairing QR code printed at startup.
    pub no_qr: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Serve(ServeOptions),
    Hooks {
        url: String,
        decision_timeout: Duration,
    },
    /// Print the pairing QR code for the web app at `url`.
    Qr {
        url: String,
    },
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliError {
    #[error("unknown command or option `{0}`")]
    Unknown(String),
    #[error("`{0}` needs a value")]
    MissingValue(&'static str),
    #[error("invalid value for `{0}`")]
    InvalidValue(&'static str),
    #[error("`{0}` and `{1}` cannot be combined")]
    Conflict(&'static str, &'static str),
}

pub fn usage() -> String {
    format!(
        "herdr-companion: answer Claude Code prompts for Herdr agents from another device\n\n\
         USAGE:\n  \
         herdr-companion serve [--all | --listen ADDR] [--decision-timeout SECS] [--herdr-socket PATH]\n  \
         \x20                       [--ntfy URL [--ntfy-details]] [--public-url URL] [--no-qr]\n  \
         herdr-companion hooks [--url URL] [--decision-timeout SECS]\n  \
         herdr-companion qr [--url URL]\n\n\
         `serve` reads its bearer token from {TOKEN_ENV} and listens on {DEFAULT_LISTEN} by default.\n\
         --all listens on every interface (port 8787). At startup it prints a QR code that opens\n\
         the web app on a phone already signed in, using --public-url, else the Tailscale or LAN\n\
         address; `qr` prints a code again.\n\
         `hooks` prints the Claude Code settings that point hooks at the companion.\n"
    )
}

pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Command, CliError> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(Command::Help);
    };
    let mut listen: SocketAddr = DEFAULT_LISTEN
        .parse()
        .map_err(|_| CliError::InvalidValue("--listen"))?;
    let mut url = None;
    let mut decision_timeout = Duration::from_secs(DEFAULT_DECISION_SECS);
    let mut herdr_socket = None;
    let mut ntfy = None;
    let mut ntfy_details = false;
    let mut public_url = None;
    let mode = match command.to_str() {
        Some("serve") => Mode::Serve,
        Some("hooks") => Mode::Hooks,
        Some("qr") => Mode::Qr,
        Some("help" | "-h" | "--help") => return Ok(Command::Help),
        _ => return Err(CliError::Unknown(command.to_string_lossy().into_owned())),
    };
    let serve = mode == Mode::Serve;
    let mut no_qr = false;
    let mut all = false;
    let mut listen_given = false;
    while let Some(arg) = args.next() {
        let mut value = |name| args.next().ok_or(CliError::MissingValue(name));
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Command::Help),
            Some("--decision-timeout") if mode != Mode::Qr => {
                let secs = value("--decision-timeout")?
                    .to_str()
                    .and_then(|secs| secs.parse::<u64>().ok())
                    .filter(|secs| (1..=3600).contains(secs))
                    .ok_or(CliError::InvalidValue("--decision-timeout"))?;
                decision_timeout = Duration::from_secs(secs);
            }
            Some("--all") if serve => all = true,
            Some("--listen") if serve => {
                listen_given = true;
                listen = value("--listen")?
                    .to_str()
                    .and_then(|addr| addr.parse().ok())
                    .ok_or(CliError::InvalidValue("--listen"))?;
            }
            // Socket paths need not be UTF-8.
            Some("--herdr-socket") if serve => {
                herdr_socket = Some(PathBuf::from(value("--herdr-socket")?))
            }
            Some("--ntfy") if serve => ntfy = Some(web_url(value("--ntfy")?, "--ntfy")?),
            Some("--ntfy-details") if serve => ntfy_details = true,
            Some("--no-qr") if serve => no_qr = true,
            Some("--public-url") if serve => {
                public_url = Some(web_url(value("--public-url")?, "--public-url")?)
            }
            Some("--url") if mode == Mode::Qr => url = Some(web_url(value("--url")?, "--url")?),
            Some("--url") if !serve => {
                url = Some(
                    value("--url")?
                        .into_string()
                        .map_err(|_| CliError::InvalidValue("--url"))?,
                );
            }
            _ => return Err(CliError::Unknown(arg.to_string_lossy().into_owned())),
        }
    }
    if all {
        if listen_given {
            return Err(CliError::Conflict("--all", "--listen"));
        }
        listen.set_ip(Ipv4Addr::UNSPECIFIED.into());
    }
    Ok(match mode {
        Mode::Serve => Command::Serve(ServeOptions {
            listen,
            decision_timeout,
            herdr_socket,
            ntfy,
            ntfy_details,
            public_url,
            no_qr,
        }),
        Mode::Hooks => Command::Hooks {
            url: url.unwrap_or_else(|| format!("http://{DEFAULT_LISTEN}/hooks/claude")),
            decision_timeout,
        },
        Mode::Qr => Command::Qr {
            url: url.unwrap_or_else(|| format!("http://{DEFAULT_LISTEN}")),
        },
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Serve,
    Hooks,
    Qr,
}

fn web_url(value: OsString, name: &'static str) -> Result<String, CliError> {
    value
        .into_string()
        .ok()
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .ok_or(CliError::InvalidValue(name))
}

/// Claude Code `settings.json` hooks for the companion. The token and pane id
/// come from the agent's environment at call time, so neither is written into
/// the settings file.
pub fn hooks_settings(url: &str, decision_timeout: Duration) -> Value {
    let handler = |timeout: u64| {
        json!([{
            "hooks": [{
                "type": "http",
                "url": url,
                "timeout": timeout,
                "headers": {
                    "Authorization": format!("Bearer ${TOKEN_ENV}"),
                    "X-Herdr-Pane": "$HERDR_PANE_ID",
                },
                "allowedEnvVars": [TOKEN_ENV, "HERDR_PANE_ID"],
            }]
        }])
    };
    json!({
        "hooks": {
            "PermissionRequest": handler(decision_timeout.as_secs() + HOOK_HEADROOM_SECS),
            "Notification": handler(5),
            "Stop": handler(5),
            "UserPromptSubmit": handler(5),
            "SessionStart": handler(5),
            // Progress during a turn. They run on every tool call, so they
            // return at once and never decide anything.
            "PreToolUse": handler(5),
            "PostToolUse": handler(5),
            "PostToolUseFailure": handler(5),
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, CliError> {
        parse_args(args.iter().map(OsString::from))
    }

    #[test]
    fn serve_defaults_to_loopback() {
        let Command::Serve(options) = parse(&["serve"]).unwrap() else {
            panic!("serve");
        };
        assert!(options.listen.ip().is_loopback());
        assert_eq!(
            options.decision_timeout,
            Duration::from_secs(DEFAULT_DECISION_SECS)
        );
        assert_eq!(options.herdr_socket, None);
        assert_eq!(options.ntfy, None);
    }

    #[test]
    fn parses_every_serve_option() {
        let command = parse(&[
            "serve",
            "--listen",
            "100.64.0.1:9000",
            "--decision-timeout",
            "30",
            "--herdr-socket",
            "/h.sock",
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Serve(ServeOptions {
                listen: "100.64.0.1:9000".parse().unwrap(),
                decision_timeout: Duration::from_secs(30),
                herdr_socket: Some("/h.sock".into()),
                ntfy: None,
                ntfy_details: false,
                public_url: None,
                no_qr: false,
            })
        );
    }

    #[test]
    fn parses_push_options() {
        let Command::Serve(options) = parse(&[
            "serve",
            "--ntfy",
            "https://ntfy.sh/t",
            "--ntfy-details",
            "--public-url",
            "https://box.ts.net",
        ])
        .unwrap() else {
            panic!("serve");
        };
        assert_eq!(options.ntfy.as_deref(), Some("https://ntfy.sh/t"));
        assert!(options.ntfy_details);
        assert_eq!(options.public_url.as_deref(), Some("https://box.ts.net"));
        assert_eq!(
            parse(&["serve", "--ntfy", "ntfy.sh/t"]),
            Err(CliError::InvalidValue("--ntfy"))
        );
        assert_eq!(
            parse(&["hooks", "--ntfy-details"]),
            Err(CliError::Unknown("--ntfy-details".into()))
        );
    }

    #[test]
    fn all_listens_on_every_interface() {
        let Command::Serve(options) = parse(&["serve", "--all"]).unwrap() else {
            panic!("serve");
        };
        assert_eq!(options.listen, "0.0.0.0:8787".parse().unwrap());
        assert_eq!(
            parse(&["serve", "--all", "--listen", "0.0.0.0:1"]),
            Err(CliError::Conflict("--all", "--listen"))
        );
        assert_eq!(
            parse(&["hooks", "--all"]),
            Err(CliError::Unknown("--all".into()))
        );
    }

    #[test]
    fn parses_pairing_options() {
        let Command::Serve(options) = parse(&["serve", "--no-qr"]).unwrap() else {
            panic!("serve");
        };
        assert!(options.no_qr);
        assert_eq!(
            parse(&["qr"]),
            Ok(Command::Qr {
                url: format!("http://{DEFAULT_LISTEN}")
            })
        );
        assert_eq!(
            parse(&["qr", "--url", "https://box.ts.net"]),
            Ok(Command::Qr {
                url: "https://box.ts.net".into()
            })
        );
        assert_eq!(
            parse(&["qr", "--decision-timeout", "5"]),
            Err(CliError::Unknown("--decision-timeout".into()))
        );
        assert_eq!(
            parse(&["hooks", "--no-qr"]),
            Err(CliError::Unknown("--no-qr".into()))
        );
    }

    #[test]
    fn rejects_bad_input_before_starting() {
        assert_eq!(
            parse(&["serve", "--listen"]),
            Err(CliError::MissingValue("--listen"))
        );
        assert_eq!(
            parse(&["serve", "--listen", "nope"]),
            Err(CliError::InvalidValue("--listen"))
        );
        assert_eq!(
            parse(&["serve", "--decision-timeout", "0"]),
            Err(CliError::InvalidValue("--decision-timeout"))
        );
        assert_eq!(
            parse(&["serve", "--url", "x"]),
            Err(CliError::Unknown("--url".into()))
        );
        assert_eq!(
            parse(&["hooks", "--listen", "x"]),
            Err(CliError::Unknown("--listen".into()))
        );
        assert_eq!(parse(&["launch"]), Err(CliError::Unknown("launch".into())));
        assert_eq!(parse(&[]), Ok(Command::Help));
        assert_eq!(parse(&["serve", "--help"]), Ok(Command::Help));
    }

    #[test]
    fn hook_settings_give_the_companion_time_to_answer_first() {
        let settings = hooks_settings("http://h/hooks/claude", Duration::from_secs(60));
        let permission = &settings["hooks"]["PermissionRequest"][0]["hooks"][0];
        assert_eq!(permission["timeout"], 70);
        assert_eq!(settings["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"], 5);
        assert_eq!(
            permission["headers"]["Authorization"],
            "Bearer $HERDR_COMPANION_TOKEN"
        );
        assert_eq!(
            permission["allowedEnvVars"],
            json!(["HERDR_COMPANION_TOKEN", "HERDR_PANE_ID"])
        );
    }
}
