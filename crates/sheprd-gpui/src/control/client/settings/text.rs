//! How settings answers read in a terminal.
use crate::control::protocol::settings::{BindingInfo, CustomInfo, Sound};
use std::fmt::Write as _;

pub(super) fn themes(themes: &[String], current: &str) -> String {
    themes
        .iter()
        .map(|name| {
            let mark = if name == current { "*" } else { " " };
            format!("{mark} {name}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether a custom command's binding runs from this client.
pub(super) fn reachable(reach: &str) -> bool {
    reach == "runs"
}

/// Why a binding does not run, as the shortcut reference says it.
pub(super) fn reach_note(reach: &str) -> &'static str {
    match reach {
        "runs" => "runs",
        "shadowed" => "used by a Herdr or Sheprd shortcut",
        "taken" => "an earlier custom command has the same keys",
        "needs_modifier" => "a key without a modifier would swallow typing",
        "no_prefix" => "no prefix key is configured",
        "over_limit" => "past the shortcuts one command may have",
        "unsupported" => "keys this client cannot type",
        _ => "does not run here",
    }
}

pub(super) fn keybinds(bindings: &[BindingInfo], custom: &[CustomInfo]) -> String {
    let mut text = String::new();
    for binding in bindings.iter().filter(|binding| !binding.keys.is_empty()) {
        let _ = writeln!(
            text,
            "{:<36}{:<28}{}",
            binding.label,
            binding.keys.join(", "),
            binding.name
        );
    }
    if !custom.is_empty() {
        text.push_str("\nHerdr custom commands\n");
        for command in custom {
            let _ = writeln!(
                text,
                "{:<36}{:<28}{}",
                command.label,
                command.keys,
                reach_note(&command.reach)
            );
        }
    }
    text.trim_end().to_owned()
}

pub(super) fn unreachable(custom: &[CustomInfo]) -> String {
    let mut text = format!(
        "{} binding{} of Herdr's custom commands cannot run from this window:",
        custom.len(),
        if custom.len() == 1 { "" } else { "s" }
    );
    for command in custom {
        let _ = write!(
            text,
            "\n  {} ({}): {}",
            command.label,
            command.keys,
            reach_note(&command.reach)
        );
    }
    text
}

/// `90` as `1m 30s`, `7200` as `2h`.
pub(super) fn duration(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    [(hours, "h"), (minutes, "m"), (seconds, "s")]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, unit)| format!("{count}{unit}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn caffeine(state: &str, seconds: Option<u64>) -> String {
    match (state, seconds) {
        ("on", Some(0)) => "Caffeine is on, ending now".to_owned(),
        ("on", Some(seconds)) => {
            format!("Caffeine is on for another {}", duration(seconds))
        }
        ("on", None) => "Caffeine is on: the display and the machine stay awake".to_owned(),
        ("pending", _) => "Caffeine is changing".to_owned(),
        ("until_agents_finish", _) => {
            "Caffeine is on until the agents finish: the display and the machine stay awake"
                .to_owned()
        }
        _ => "Caffeine is off".to_owned(),
    }
}

pub(super) fn sound_use(sound: Sound) -> &'static str {
    match sound {
        Sound::Done => "when an agent finishes",
        Sound::Request => "when an agent needs you",
    }
}

pub(super) fn update(state: &str, latest: Option<&str>, detail: Option<&str>) -> String {
    let latest = latest.unwrap_or("A newer release");
    match state {
        "disabled" => format!(
            "Updates are off in this build: {}",
            detail.unwrap_or("no reason given")
        ),
        "idle" => "No update check has finished yet".to_owned(),
        "checking" => "Checking for the latest release".to_owned(),
        "current" => "Sheprd is up to date".to_owned(),
        "available" => format!("{latest} is available; update from Sheprd"),
        "downloading" => format!("{latest} is downloading"),
        "ready" => format!("{latest} is downloaded; install it from Sheprd"),
        "homebrew" => format!("{latest} is available; upgrade it with Homebrew"),
        "restart" => format!("{latest} is installed; restart Sheprd to use it"),
        "error" => format!(
            "The last update check failed: {}",
            detail.unwrap_or("unknown error")
        ),
        state => match detail {
            Some(detail) => format!("Updates: {state} ({detail})"),
            None => format!("Updates: {state}"),
        },
    }
}
