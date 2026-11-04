//! The example config's usage provider section, generated from each
//! provider's settings and the English of their messages, and every
//! setting's message checked against `usage-<id>.ftl`.
use super::*;
use crate::i18n;

const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/config-gpui.example.toml");
const START: &str = "# --- usage providers: generated from each provider's settings ---\n";
const END: &str = "# --- end usage providers ---\n";

/// The provider section: what each provider reads and how to find it.
fn docs() -> String {
    let mut docs = String::from(START);
    let (configurable, automatic): (Vec<_>, Vec<_>) =
        registry::all().partition(|provider| !provider.service().meta().settings.is_empty());
    docs.push_str("#\n# Found from the agent's own sign-in on the host, with nothing to set:\n");
    let ids: Vec<_> = automatic.iter().map(|provider| provider.id()).collect();
    for line in wrap(&ids.join(", "), 76) {
        docs.push_str(&format!("#   {line}\n"));
    }
    for provider in configurable {
        let meta = provider.service().meta();
        docs.push_str(&format!(
            "#\n# {} ({})\n# [usage.providers.{}]\n",
            meta.name, meta.id, meta.id
        ));
        for setting in meta.settings {
            for line in wrap(&i18n::english(setting.message, &[]), 76) {
                docs.push_str(&format!("#   {line}\n"));
            }
            let variables = if setting.env.is_empty() {
                String::new()
            } else {
                format!("  # or {}", setting.env.join(", "))
            };
            docs.push_str(&format!("# {} = \"\"{variables}\n", setting.name));
        }
    }
    docs.push_str(END);
    docs
}

/// The generated section as it stands in `text`, markers included.
fn docs_in(text: &str) -> Option<&str> {
    let start = text.find(START)?;
    let end = text[start..].find(END)? + start + END.len();
    Some(&text[start..end])
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[test]
fn every_setting_names_a_message_without_arguments() {
    for provider in registry::all() {
        for setting in provider.service().meta().settings {
            assert_eq!(
                i18n::takes(setting.message),
                Some(&[][..]),
                "{}.{}: {}",
                provider.id(),
                setting.name,
                setting.message
            );
        }
    }
}

/// Regenerate with `HERDR_BLESS_EXAMPLE=1 cargo test example_config_documents`.
#[test]
fn example_config_documents_every_provider_setting() {
    // A Windows checkout may carry CRLF line endings.
    let text = std::fs::read_to_string(PATH).unwrap().replace("\r\n", "\n");
    let expected = docs();
    if std::env::var_os("HERDR_BLESS_EXAMPLE").is_some() {
        let updated = match docs_in(&text) {
            Some(current) => text.replace(current, &expected),
            None => format!("{}\n\n{expected}", text.trim_end_matches('\n')),
        };
        std::fs::write(PATH, updated).unwrap();
        return;
    }
    assert_eq!(
        docs_in(&text),
        Some(expected.as_str()),
        "config-gpui.example.toml is stale; rerun with HERDR_BLESS_EXAMPLE=1"
    );
}
