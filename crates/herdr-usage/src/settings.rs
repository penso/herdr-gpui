//! `[usage]` display switches and provider settings.

use super::{model::Provider, registry};
use secrecy::SecretString;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UsageConfig {
    pub show: bool,
    pub topbar: bool,
    pub inline: bool,
    /// Provider ids shown even when this machine has no sign-in for them.
    pub show_providers: Vec<String>,
    /// Provider ids never shown, even when detected.
    pub hide_providers: Vec<String>,
    /// Whether providers listed in `show_providers` that sign in with a web
    /// session may read it from Chrome or Safari. Reading Chrome's cookies
    /// asks for Keychain access once.
    pub browser_cookies: bool,
    pub providers: BTreeMap<String, ProviderSettings>,
}

impl Default for UsageConfig {
    fn default() -> Self {
        Self {
            show: true,
            topbar: true,
            inline: true,
            show_providers: Vec::new(),
            hide_providers: Vec::new(),
            browser_cookies: true,
            providers: BTreeMap::new(),
        }
    }
}

/// One provider's table, e.g. `[usage.providers.openrouter]`. Values are kept
/// as secrets, since most are keys or cookies; names are checked against what
/// the provider declares.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct ProviderSettings(BTreeMap<String, SecretString>);

impl ProviderSettings {
    pub fn get(&self, name: &str) -> Option<&SecretString> {
        self.0.get(name)
    }

    #[cfg(test)]
    pub fn with(mut self, name: &str, value: &str) -> Self {
        self.0.insert(name.to_owned(), value.into());
        self
    }
}

impl UsageConfig {
    /// Drops provider ids and setting names this build does not know and
    /// returns their key paths, so a config naming a provider from a newer
    /// build still loads.
    pub fn retain_known(&mut self) -> Vec<String> {
        let mut unknown = Vec::new();
        for (key, ids) in [
            ("show_providers", &mut self.show_providers),
            ("hide_providers", &mut self.hide_providers),
        ] {
            ids.retain(|id| {
                let known = registry::find(id).is_some();
                if !known {
                    unknown.push(format!("usage.{key}.{id}"));
                }
                known
            });
        }
        self.providers.retain(|id, settings| {
            let Some(provider) = registry::find(id) else {
                unknown.push(format!("usage.providers.{id}"));
                return false;
            };
            let declared = provider.service().meta().settings;
            settings.0.retain(|name, _| {
                let known = declared.iter().any(|setting| setting.name == name.as_str());
                if !known {
                    unknown.push(format!("usage.providers.{id}.{name}"));
                }
                known
            });
            true
        });
        unknown
    }

    pub fn settings(&self, provider: Provider) -> Option<&ProviderSettings> {
        self.providers.get(provider.id())
    }

    pub fn shown(&self, provider: Provider) -> bool {
        self.show_providers.iter().any(|id| id == provider.id())
    }

    pub fn hidden(&self, provider: Provider) -> bool {
        self.hide_providers.iter().any(|id| id == provider.id())
    }
}
