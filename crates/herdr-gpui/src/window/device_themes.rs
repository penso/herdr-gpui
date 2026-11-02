//! The terminal's colors per saved device (`[devices.<id>] theme`). Only the
//! terminal changes: the chrome keeps the app theme. Every terminal a window
//! paints belongs to its selected endpoint, parked groups included, so one
//! theme serves the whole terminal area.
// i18n: migrated
use crate::i18n::{Localize, fl};
use crate::{
    HerdrWindow,
    config::{Config, Theme},
    contrast::Contrast,
};
use gpui::{Context, Task};
use std::collections::BTreeMap;

/// What a set of resolved themes was read for.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    light: bool,
    contrast: Contrast,
    /// The config load it was read for, so every reload reads the files
    /// again: one edited in place, or missing before, is picked up then.
    revision: u64,
    /// Each device's `theme` value, by profile ID, in profile order.
    themes: Vec<(String, String)>,
}

impl Key {
    fn matches(&self, config: &Config, light: bool, revision: u64) -> bool {
        self.light == light
            && self.revision == revision
            && self.contrast == config.contrast
            && self
                .themes
                .iter()
                .map(|(profile, theme)| (profile.as_str(), theme.as_str()))
                .eq(config.device_themes())
    }

    fn new(config: &Config, light: bool, revision: u64) -> Self {
        Self {
            light,
            contrast: config.contrast,
            revision,
            themes: config
                .device_themes()
                .map(|(profile, theme)| (profile.to_owned(), theme.to_owned()))
                .collect(),
        }
    }
}

/// Device themes resolved off the UI thread, since a theme may be a file.
#[derive(Default)]
pub(crate) struct DeviceThemes {
    resolved: Option<Key>,
    themes: BTreeMap<String, Theme>,
    /// The key being read, and its task.
    load: Option<(Key, Task<()>)>,
    /// A choice being saved, shown until the config and its read catch up.
    pinned: Option<Pinned>,
    /// Numbers each save, so a finished write only updates its own pin.
    saves: u64,
}

/// A device theme saved from the menu. The write, the reload after it, and
/// the read of its colors each take a moment; without the pin the terminal
/// would show the old colors in between.
struct Pinned {
    endpoint: String,
    /// The saved value, `None` for the app theme.
    value: Option<String>,
    /// Its colors, `None` for the app theme.
    theme: Option<Theme>,
    save: u64,
    /// `None` while the write runs, then what was loading when it ended.
    written: Option<Written>,
}

/// The config loads around a finished write.
#[derive(Clone, Copy)]
struct Written {
    /// `config_load_revision` when the write finished. Only loads after it
    /// can hold the saved value.
    revision: u64,
    /// Whether a load was running then. It may have read the file before
    /// or after the write, so only its saved value is trusted, and an old
    /// one waits for the reload after it.
    busy: bool,
}

impl DeviceThemes {
    /// The device's resolved theme. One that failed to load has none, and
    /// the terminal keeps the app theme.
    fn get(&self, profile: &str) -> Option<&Theme> {
        self.themes.get(profile)
    }
}

/// Reads every theme in `key`. A theme that cannot be read is logged and
/// left out rather than failing the others.
fn resolve(config: &Config, key: &Key) -> BTreeMap<String, Theme> {
    key.themes
        .iter()
        .filter_map(|(profile, value)| match config.theme_named(value, key.light) {
            Ok(theme) => Some((profile.clone(), theme)),
            Err(error) => {
                tracing::warn!(profile, theme = value, %error, "Could not load the device theme");
                None
            }
        })
        .collect()
}

impl HerdrWindow {
    /// Brings the resolved device themes in line with the config and the
    /// system appearance. Called while rendering; it only compares until
    /// either changes, and the files are read on the background executor.
    pub(super) fn sync_device_themes(&mut self, cx: &mut Context<Self>) {
        let light = crate::app::light_appearance(cx);
        self.release_device_theme_pin(light);
        let revision = self.config_load_revision;
        let state = &self.device_themes;
        if state
            .load
            .as_ref()
            .map(|(key, _)| key)
            .or(state.resolved.as_ref())
            .is_some_and(|key| key.matches(&self.config, light, revision))
        {
            return;
        }
        let key = Key::new(&self.config, light, revision);
        if key.themes.is_empty() {
            let state = &mut self.device_themes;
            state.resolved = Some(key);
            state.themes.clear();
            state.load = None;
            return;
        }
        let config = self.config.clone();
        let read = cx.background_executor().spawn({
            let key = key.clone();
            async move {
                let themes = resolve(&config, &key);
                (key, themes)
            }
        });
        let task = cx.spawn(async move |this, cx| {
            let (key, themes) = read.await;
            let _ = this.update(cx, |this, cx| {
                // A newer key replaced this read; its own result applies.
                if this
                    .device_themes
                    .load
                    .as_ref()
                    .is_none_or(|(loading, _)| *loading != key)
                {
                    return;
                }
                this.device_themes.resolved = Some(key);
                this.device_themes.themes = themes;
                this.device_themes.load = None;
                cx.notify();
            });
        });
        self.device_themes.load = Some((key, task));
    }

    /// The value the device shows: the one being saved, else the config's.
    pub(crate) fn device_theme_value(&self, endpoint: &str) -> Option<&str> {
        match &self.device_themes.pinned {
            Some(pinned) if pinned.endpoint == endpoint => pinned.value.as_deref(),
            _ => self.config.device_theme(endpoint),
        }
    }

    /// Writes the device's terminal theme off the UI thread and reloads,
    /// showing `theme` on its terminal until the reload and its read land.
    /// `theme` is the loaded colors of `value`, `None` for the app theme.
    pub(crate) fn save_device_theme(
        &mut self,
        endpoint: String,
        profile: String,
        value: Option<String>,
        theme: Option<Theme>,
        cx: &mut Context<Self>,
    ) {
        let saved = value.clone();
        self.save_device_theme_with(
            endpoint,
            value,
            theme,
            move || Config::save_device_theme(&profile, saved.as_deref()),
            cx,
        );
    }

    fn save_device_theme_with(
        &mut self,
        endpoint: String,
        value: Option<String>,
        theme: Option<Theme>,
        write: impl FnOnce() -> crate::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.theme_save_in_flight() {
            return;
        }
        let state = &mut self.device_themes;
        state.saves += 1;
        let save = state.saves;
        state.pinned = Some(Pinned {
            endpoint,
            value,
            theme,
            save,
            written: None,
        });
        let written = cx.background_executor().spawn(async move { write() });
        cx.spawn(async move |this, cx| {
            let result = written.await;
            let _ = this.update(cx, |this, cx| {
                let pinned = this
                    .device_themes
                    .pinned
                    .as_mut()
                    .filter(|pinned| pinned.save == save);
                match result {
                    Ok(()) => {
                        if let Some(pinned) = pinned {
                            pinned.written = Some(Written {
                                revision: this.config_load_revision,
                                busy: this.config_load.is_some(),
                            });
                        }
                        // Refused while a load runs; the config watcher then
                        // reloads once it ends, as for other saves.
                        this.load_gui_config(cx);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Could not save the device theme");
                        if pinned.is_some() {
                            this.device_themes.pinned = None;
                        }
                        this.local_error = Some(fl!(
                            "window-device-themes-save-failed",
                            error = error.localize()
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Drops the pin once a load after the write has landed and either the
    /// config says what was saved and its read is done, or a load that
    /// certainly read the file after the write says something else, as
    /// after a later edit, and the pin would only hide it.
    fn release_device_theme_pin(&mut self, light: bool) {
        let Some(pinned) = &self.device_themes.pinned else {
            return;
        };
        let Some(written) = pinned.written else {
            return;
        };
        let revision = self.config_load_revision;
        let loads = revision.wrapping_sub(written.revision);
        if loads == 0 {
            return;
        }
        let saved = self.config.device_theme(&pinned.endpoint) == pinned.value.as_deref();
        let release = if saved {
            self.device_themes
                .resolved
                .as_ref()
                .is_some_and(|key| key.matches(&self.config, light, revision))
        } else {
            loads > u64::from(written.busy)
        };
        if release {
            self.device_themes.pinned = None;
        }
    }

    /// The colors the terminal area paints with: the selected device's own
    /// theme while it has one, else the app theme.
    pub(crate) fn terminal_theme(&self) -> &Theme {
        let Some(endpoint) = self.endpoints.get(self.selected_endpoint) else {
            return &self.theme;
        };
        if let Some(preview) = self.device_theme_preview(&endpoint.id) {
            return preview;
        }
        if let Some(pinned) = self
            .device_themes
            .pinned
            .as_ref()
            .filter(|pinned| pinned.endpoint == endpoint.id)
        {
            return pinned.theme.as_ref().unwrap_or(&self.theme);
        }
        // A theme removed from the config stops at once, without waiting
        // for the read that drops it.
        if self.config.device_theme(&endpoint.id).is_none() {
            return &self.theme;
        }
        crate::endpoint::saved_profile_id(&endpoint.id)
            .and_then(|profile| self.device_themes.get(profile))
            .unwrap_or(&self.theme)
    }
}

#[cfg(test)]
mod tests;
