//! Plan usage in the status bar, its panel, and the Usage section. Reading
//! it is `herdr-usage`'s; this draws it, keeps the Keychain grants the user
//! gave, and embeds the provider logos.
// i18n: migrated

mod access;
mod details;
mod icons;
mod panel;
mod render;
mod section;
pub(crate) mod text;
mod ui;

#[cfg(test)]
mod tests;

pub(crate) use access::KeychainGrants;
pub(crate) use herdr_usage::{
    Access, Host, Provider, Reading, Shell, Usage, UsageConfig, model, registry, service,
};
pub(crate) use panel::PANEL_WIDTH;
pub(crate) use render::Hint;

pub(crate) fn icon(path: &str) -> Option<&'static [u8]> {
    icons::PROVIDER_ICONS
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| *bytes)
}

pub(crate) fn icon_paths() -> impl Iterator<Item = &'static str> {
    icons::PROVIDER_ICONS.iter().map(|(name, _)| *name)
}
