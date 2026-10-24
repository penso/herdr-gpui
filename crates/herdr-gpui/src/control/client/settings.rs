//! `config`, `theme`, `font`, `keybinds`, `caffeine`, `sound`, `bell`,
//! `update`, and `integrations`. What only reads or writes files is done
//! here, whether or not Herdr UI runs; the rest asks the app.
mod config;
#[cfg(test)]
mod tests;
mod text;

use super::{Context, Output, unexpected};
use crate::{
    cli::control::{
        CaffeineCommand, IntegrationsCommand, KeybindsCommand, SoundCommand, ThemeCommand,
        UpdateCommand,
    },
    config::Config,
    control::{
        ErrorCode,
        protocol::{
            Request, Response,
            settings::{MAX_CAFFEINE_SECONDS, Reply, SettingsOp, Sound},
        },
    },
};
pub(super) use config::config;
use serde_json::json;

/// No app to ask: it is not running.
fn not_running(error: &crate::Error) -> bool {
    matches!(
        error,
        crate::Error::Control(crate::control::Error::Unavailable { .. })
    )
}

fn refused(code: ErrorCode, message: impl Into<String>) -> crate::Error {
    crate::control::Error::Rejected {
        code,
        message: message.into(),
    }
    .into()
}

fn settings(context: &Context, op: SettingsOp) -> crate::Result<Reply> {
    match context.call(Request::Settings, op)? {
        Response::Settings { reply } => Ok(reply),
        response => Err(unexpected(response)),
    }
}

pub(super) fn theme(command: ThemeCommand, context: &Context) -> crate::Result<Output> {
    match command {
        ThemeCommand::List => {
            // Listing needs only the selected theme, to mark it; settings
            // that do not load still leave every theme to choose from.
            let config = Config::check_files().unwrap_or_default();
            let themes = config.available_themes()?;
            Output::data(
                text::themes(&themes, &config.theme),
                &json!({ "current": config.theme, "themes": themes }),
            )
        }
        ThemeCommand::Set(None) => Err(refused(ErrorCode::InvalidRequest, "Name a theme")),
        ThemeCommand::Set(Some(name)) => {
            match context.act(Request::Settings, SettingsOp::Theme { name: name.clone() }) {
                // Saving needs no app: it applies the file when it starts.
                Err(error) if not_running(&error) => {
                    Config::check_files()?.save_theme(&name)?;
                    Ok(Output::message(format!(
                        "Theme set to {name}; Herdr UI shows it when it starts"
                    )))
                }
                result => result,
            }
        }
    }
}

pub(super) fn font(command: ThemeCommand, context: &Context) -> crate::Result<Output> {
    match command {
        ThemeCommand::List => match settings(context, SettingsOp::Fonts)? {
            Reply::Fonts { fonts } => Output::data(fonts.join("\n"), &fonts),
            reply => Err(unexpected(Response::Settings { reply })),
        },
        ThemeCommand::Set(name) => context.act(Request::Settings, SettingsOp::Font { name }),
    }
}

pub(super) fn keybinds(command: KeybindsCommand, context: &Context) -> crate::Result<Output> {
    let (bindings, custom) = match settings(context, SettingsOp::Keybinds)? {
        Reply::Keybinds { bindings, custom } => (bindings, custom),
        reply => return Err(unexpected(Response::Settings { reply })),
    };
    match command {
        KeybindsCommand::List => Output::data(
            text::keybinds(&bindings, &custom),
            &json!({ "bindings": bindings, "custom": custom }),
        ),
        KeybindsCommand::Check => {
            let unreachable: Vec<_> = custom
                .into_iter()
                .filter(|command| !text::reachable(&command.reach))
                .collect();
            if unreachable.is_empty() {
                return Ok(Output::message(
                    "Every binding of Herdr's custom commands reaches this window",
                ));
            }
            Err(refused(ErrorCode::Failed, text::unreachable(&unreachable)))
        }
    }
}

pub(super) fn caffeine(command: CaffeineCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        CaffeineCommand::Status => SettingsOp::Caffeine {
            switch: None,
            seconds: None,
        },
        CaffeineCommand::Set { seconds, .. }
            if seconds.is_some_and(|seconds| seconds > MAX_CAFFEINE_SECONDS) =>
        {
            return Err(refused(ErrorCode::InvalidRequest, "--for is at most 24h"));
        }
        CaffeineCommand::Set { switch, seconds } => SettingsOp::Caffeine {
            switch: Some(switch),
            seconds,
        },
    };
    match settings(context, op)? {
        Reply::Caffeine { state, seconds } => Output::data(
            text::caffeine(&state, seconds),
            &json!({ "state": state, "seconds": seconds }),
        ),
        reply => Err(unexpected(Response::Settings { reply })),
    }
}

pub(super) fn sound(command: SoundCommand, context: &Context) -> crate::Result<Output> {
    match command {
        SoundCommand::List => {
            let sounds: Vec<_> = Sound::ALL
                .iter()
                .map(|sound| json!({ "name": sound.name(), "plays": text::sound_use(*sound) }))
                .collect();
            let listed = Sound::ALL
                .iter()
                .map(|sound| format!("{:<10}{}", sound.name(), text::sound_use(*sound)))
                .collect::<Vec<_>>()
                .join("\n");
            Output::data(listed, &sounds)
        }
        SoundCommand::Play(sound) => context.act(Request::Settings, SettingsOp::Sound { sound }),
    }
}

pub(super) fn bell(context: &Context) -> crate::Result<Output> {
    context.act(Request::Settings, SettingsOp::Bell)
}

pub(super) fn update(command: UpdateCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        UpdateCommand::Status => SettingsOp::Update,
        UpdateCommand::Check => SettingsOp::CheckUpdate,
    };
    match settings(context, op)? {
        Reply::Update {
            state,
            latest,
            detail,
        } => Output::data(
            text::update(&state, latest.as_deref(), detail.as_deref()),
            &json!({ "state": state, "latest": latest, "detail": detail }),
        ),
        reply => Err(unexpected(Response::Settings { reply })),
    }
}

pub(super) fn integrations(
    command: IntegrationsCommand,
    context: &Context,
) -> crate::Result<Output> {
    match command {
        IntegrationsCommand::List => match settings(context, SettingsOp::Integrations)? {
            Reply::Integrations { integrations } => {
                let mut listed = integrations
                    .iter()
                    .map(|row| format!("{:<28}{:<16}{}", row.label, row.status, row.id))
                    .collect::<Vec<_>>()
                    .join("\n");
                if integrations.is_empty() {
                    listed = "The host offers no integrations".into();
                }
                Output::data(listed, &integrations)
            }
            reply => Err(unexpected(Response::Settings { reply })),
        },
        IntegrationsCommand::InstallSkill => install_skill(),
    }
}

/// Installs the agent skill as the app's offer does, and records the choice
/// so the app keeps it current and does not ask again.
fn install_skill() -> crate::Result<Output> {
    use crate::agent_skill::{self, Choice};
    let home = crate::config::home()?;
    let text = agent_skill::text(agent_skill::executable().as_deref());
    let paths = agent_skill::install(&home, &text)?;
    let mut message = String::from("Installed the agent skill at:");
    for path in &paths {
        message.push_str(&format!("\n  {}", path.display()));
    }
    if let Err(error) = agent_skill::save_choice(Choice::Installed) {
        message.push_str(&format!(
            "\nHerdr UI may still offer it, as the choice was not saved: {error}"
        ));
    }
    let paths: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    Output::data(message, &json!({ "paths": paths }))
}
