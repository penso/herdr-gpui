//! Every provider has an icon the app embeds, or names a generic agent mark.
use super::*;

#[test]
fn every_provider_has_an_embedded_icon() {
    for provider in registry::all() {
        let icon = crate::usage::icons::path(provider);
        assert!(
            crate::usage::icon(icon).is_some() || icon.starts_with("icons/agent-"),
            "{} has no icon at {icon}",
            provider.id()
        );
    }
}
