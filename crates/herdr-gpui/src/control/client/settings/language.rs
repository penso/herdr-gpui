//! `language`: the locale the windows show, the languages there are, and
//! the `language` setting, which is a file edit made here.
use super::{Context, Output, not_running, settings, unexpected};
use crate::{
    cli::control::LanguageCommand,
    config::Config,
    control::protocol::{
        Response,
        settings::{LanguageInfo, Reply, SettingsOp},
    },
};
use herdr_i18n::{Language, Pseudo};
use serde_json::json;

pub(in crate::control::client) fn language(
    command: LanguageCommand,
    context: &Context,
) -> crate::Result<Output> {
    match command {
        LanguageCommand::Status => status(context),
        LanguageCommand::List => list(),
        LanguageCommand::Set(language) => {
            Config::set_key("language", toml_edit::Value::from(language.to_string()))?;
            Ok(Output::message(format!(
                "language = \"{language}\" in config-gpui.local.toml; Herdr UI applies it at once"
            )))
        }
    }
}

fn status(context: &Context) -> crate::Result<Output> {
    let (info, running) = match settings(context, SettingsOp::Language) {
        Ok(Reply::Language(info)) => (info, true),
        Ok(reply) => return Err(unexpected(Response::Settings { reply })),
        // What it would show: the setting, resolved as the app resolves it.
        Err(error) if not_running(&error) => (local(&Config::check_files()?.language), false),
        Err(error) => return Err(error),
    };
    Output::data(
        text(&info, running),
        &json!({
            "configured": info.configured,
            "pseudo": info.pseudo,
            "locale": info.locale,
            "direction": info.direction,
            "running": running,
        }),
    )
}

pub(super) fn local(language: &Language) -> LanguageInfo {
    let locale = language.resolve(&crate::i18n::available(), &herdr_i18n::system_languages());
    LanguageInfo {
        configured: language.to_string(),
        pseudo: None,
        direction: locale.direction().name().to_owned(),
        locale: locale.to_string(),
    }
}

pub(super) fn text(info: &LanguageInfo, running: bool) -> String {
    let shows = if running {
        "Herdr UI shows"
    } else {
        "Herdr UI would show"
    };
    let mut text = format!(
        "language = {}\n{shows} {} ({})",
        info.configured, info.locale, info.direction
    );
    if let Some(pseudo) = &info.pseudo {
        text.push_str(&format!(", the QA pseudo-locale {pseudo}"));
    }
    text
}

fn list() -> crate::Result<Output> {
    let languages: Vec<_> = crate::i18n::available()
        .into_iter()
        .map(|locale| (locale, false))
        .chain(Pseudo::ALL.map(|pseudo| (pseudo.locale(), true)))
        .collect();
    let text = languages
        .iter()
        .map(|(locale, pseudo)| {
            let note = if *pseudo {
                "  pseudo-locale for testing"
            } else {
                ""
            };
            format!(
                "{:<8}{}{note}",
                locale.to_string(),
                locale.direction().name()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let languages: Vec<_> = languages
        .iter()
        .map(|(locale, pseudo)| {
            json!({
                "tag": locale.to_string(),
                "direction": locale.direction().name(),
                "pseudo": pseudo,
            })
        })
        .collect();
    Output::data(text, &languages)
}
