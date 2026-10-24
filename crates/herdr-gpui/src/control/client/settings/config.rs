//! `config`: the settings files, read and changed here. The app's config
//! watcher applies a saved change on its own, so only `reload` asks it, and
//! `check` asks it about fonts when it runs.
use super::{Context, Output, not_running, refused, settings};
use crate::{
    cli::control::ConfigCommand,
    config::{Config, Layer, daemon_config_path, parse_value},
    control::{
        ErrorCode,
        protocol::{
            Request,
            settings::{Reply, SettingsOp},
        },
    },
};
use serde_json::json;

const LOCAL: &str = "config-gpui.local.toml";

pub(in crate::control::client) fn config(
    command: ConfigCommand,
    context: &Context,
) -> crate::Result<Output> {
    match command {
        ConfigCommand::Path => paths(),
        ConfigCommand::Get(key) => get(&key),
        ConfigCommand::Set { key, value } => {
            let value = parse_value(&value);
            Config::set_key(&key, value.clone())?;
            Ok(Output::message(format!(
                "{key} = {} in {LOCAL}; Herdr UI applies it at once",
                value.to_string().trim()
            )))
        }
        ConfigCommand::Unset(key) => Ok(if Config::unset_key(&key)? {
            Output::message(format!("Removed {key} from {LOCAL}; the default applies"))
        } else {
            Output::message(format!("{key} is not set in {LOCAL}")).nothing()
        }),
        ConfigCommand::Reload => context.act(Request::Settings, SettingsOp::ReloadConfig),
        ConfigCommand::Check => check(context),
    }
}

fn paths() -> crate::Result<Output> {
    let managed = Config::path()?;
    let local = Config::local_path()?;
    let herdr = daemon_config_path(herdr_client::local_var);
    Output::data(
        format!(
            "{}\n    your settings, which Herdr UI also saves its choices to\n{}\n    the defaults, rewritten by each version\n{}\n    Herdr's own config, of which Herdr UI honors a few keys",
            local.display(),
            managed.display(),
            herdr.display(),
        ),
        &json!({
            "local": local.display().to_string(),
            "managed": managed.display().to_string(),
            "herdr": herdr.display().to_string(),
        }),
    )
}

fn get(key: &str) -> crate::Result<Output> {
    let Some((value, layer)) = Config::effective_value(key)? else {
        return Err(refused(
            ErrorCode::NotFound,
            format!("{key} is not set; config-gpui.toml lists every setting"),
        ));
    };
    let source = match layer {
        Layer::Local => "local",
        Layer::Default => "default",
    };
    Output::data(
        value.to_string(),
        &json!({ "key": key, "value": value, "source": source }),
    )
}

/// The problems loading would find, and the configured fonts the running
/// app does not have. Problems are an error, so the exit status is 1.
fn check(context: &Context) -> crate::Result<Output> {
    let mut problems = Vec::new();
    match Config::check_files() {
        Ok(config) => problems.extend(config.diagnostic()),
        Err(error) => problems.push(error.to_string()),
    }
    let fonts_checked = match settings(context, SettingsOp::CheckConfig) {
        Ok(Reply::Config { missing_fonts }) => {
            if !missing_fonts.is_empty() {
                problems.push(format!(
                    "Configured fonts not installed: {}",
                    missing_fonts.join(", ")
                ));
            }
            true
        }
        Ok(reply) => {
            return Err(super::unexpected(
                crate::control::protocol::Response::Settings { reply },
            ));
        }
        // No app, or no window to ask: the files alone were checked.
        Err(error)
            if not_running(&error)
                || matches!(
                    error,
                    crate::Error::Control(crate::control::Error::Rejected {
                        code: ErrorCode::NoWindow,
                        ..
                    })
                ) =>
        {
            false
        }
        Err(error) => return Err(error),
    };
    let fonts = if fonts_checked {
        ""
    } else {
        "\nFonts were not checked: that needs a running Herdr UI window."
    };
    if problems.is_empty() {
        return Ok(Output::message(format!("No problems found{fonts}")));
    }
    Err(refused(
        ErrorCode::Failed,
        format!("{}{fonts}", problems.join("\n")),
    ))
}
