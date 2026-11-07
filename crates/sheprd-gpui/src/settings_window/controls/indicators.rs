//! Agent status indicators: which of this client's sets draws status, and
//! the edge rail. The Indicators page renders the choice; this keeps how a
//! pick is saved.
// i18n: migrated
use super::*;
use crate::config::{StatusIcons, preferences::Preference};

/// The messages naming a set on the Indicators page, and saying in one
/// line what sets it apart, under its preview.
pub(in crate::settings_window) const fn set_messages(
    icons: StatusIcons,
) -> (&'static str, &'static str) {
    match icons {
        StatusIcons::Dots => (
            "settings-indicators-set-dots",
            "settings-indicators-set-dots-description",
        ),
        StatusIcons::Solid => (
            "settings-indicators-set-solid",
            "settings-indicators-set-solid-description",
        ),
        StatusIcons::Outline => (
            "settings-indicators-set-outline",
            "settings-indicators-set-outline-description",
        ),
        StatusIcons::Tiles => (
            "settings-indicators-set-tiles",
            "settings-indicators-set-tiles-description",
        ),
        StatusIcons::Wave => (
            "settings-indicators-set-wave",
            "settings-indicators-set-wave-description",
        ),
        StatusIcons::Braille => (
            "settings-indicators-set-braille",
            "settings-indicators-set-braille-description",
        ),
        StatusIcons::Orca => (
            "settings-indicators-set-orca",
            "settings-indicators-set-orca-description",
        ),
    }
}

impl SettingsWindow {
    /// Picks the set status is drawn with. The set already in use saves
    /// nothing. A retired `rail` value turned the rail on with no `edge_rail`
    /// key, so an on rail is written with the set to stay on once that value
    /// is replaced.
    pub(in crate::settings_window) fn choose_status_icons(
        &mut self,
        icons: StatusIcons,
        cx: &mut Context<Self>,
    ) {
        if self.config.status_icons == icons {
            return;
        }
        let edits: Vec<Preference> = self
            .config
            .edge_rail
            .then_some(Preference::EdgeRail(true))
            .into_iter()
            .chain([Preference::StatusIcons(icons)])
            .collect();
        #[cfg(test)]
        if let Some(io) = self.controls.preference_io.clone() {
            let load = io.load.clone();
            self.save_with(
                move || edits.into_iter().try_for_each(|edit| (io.write)(edit)),
                move || load(),
                false,
                cx,
            );
            return;
        }
        self.save_native(
            move || edits.into_iter().try_for_each(Config::save_preference),
            cx,
        );
    }
}
