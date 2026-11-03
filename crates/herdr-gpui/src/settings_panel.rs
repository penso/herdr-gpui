//! Herdr's shared settings as this window loaded them, read off the UI thread.
// i18n: migrated
use crate::{
    HerdrWindow,
    herdr_settings::Settings,
    i18n::{Localize, fl},
};
use gpui::*;

/// The theme setting's value that shows the daemon's theme.
const FOLLOW_HERDR: &str = "Follow Herdr"; // i18n: data

#[derive(Default)]
pub(crate) struct SettingsPanel {
    pub(crate) shared: Option<Settings>,
    pub(crate) task: Option<Task<()>>,
    pub(crate) loaded: bool,
    pub(crate) error: Option<String>,
}

impl HerdrWindow {
    pub(crate) fn load_shared_settings(&mut self, cx: &mut Context<Self>) {
        self.load_shared_settings_with(Settings::load, cx);
    }

    fn load_shared_settings_with(
        &mut self,
        load: impl FnOnce() -> crate::Result<Settings> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.settings.task.is_some() {
            return;
        }
        self.settings.error = None;
        let load = cx.background_executor().spawn(async move { load() });
        self.settings.task = Some(cx.spawn(async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                this.settings.task = None;
                this.settings.loaded = true;
                match result {
                    Ok(shared) => {
                        this.settings.shared = Some(shared);
                        this.apply_sidebar_start();
                        this.apply_shared_theme(cx);
                        this.apply_shared_agent_sort();
                        this.reload_notification_config(cx);
                    }
                    Err(error) => {
                        this.settings.error =
                            Some(fl!("settings-panel-load-failed", error = error.localize()));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Follows the daemon's `ui.agent_panel_sort` on every load, so a config
    /// reload changes the order too, unless the user toggled the panel.
    pub(crate) fn apply_shared_agent_sort(&mut self) {
        if !self.agent_sort_modified
            && let Some(shared) = &self.settings.shared
        {
            self.agent_sort = shared.agent_sort;
        }
    }

    pub(crate) fn apply_shared_theme(&mut self, cx: &mut Context<Self>) {
        if self.config.theme == FOLLOW_HERDR
            && self.menu.page != Some(crate::menu::Page::Themes)
            && !self.theme_save_in_flight()
            && let Some(shared) = &self.settings.shared
        {
            let light = matches!(
                cx.window_appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            );
            match shared.theme(light) {
                Ok(theme) => {
                    self.theme = theme.styled_for(&self.config);
                    if let Some(mut appearance) =
                        cx.try_global::<crate::app::InitialAppearance>().cloned()
                        && appearance.config.theme == FOLLOW_HERDR
                    {
                        appearance.theme = self.theme.clone();
                        cx.set_global(appearance);
                    }
                    crate::log_window::set_appearance(&self.config, &self.theme, cx);
                }
                Err(error) => self.settings.error = Some(error.localize()),
            }
        }
    }

    /// Follows the system appearance: reads the theme again, for the other
    /// side of a `light:…,dark:…` pair or for a theme file that a desktop
    /// theme switcher rewrote before it changed the appearance. Herdr hears
    /// of the new appearance only with the colors loaded for it.
    pub(crate) fn apply_system_theme(&mut self, cx: &mut Context<Self>) {
        // Follow Herdr already shows the daemon's theme for it.
        if self.config.theme == FOLLOW_HERDR || !self.reload_theme(cx) {
            self.theme_light = crate::app::light_appearance(cx);
        }
    }

    /// Reads the configured theme again, for the system appearance. Theme
    /// files are read on the background executor; a result is dropped when
    /// the theme or appearance changed while it loaded. Returns `false`
    /// without reading while the picker or Settings owns the theme.
    pub(crate) fn reload_theme(&mut self, cx: &mut Context<Self>) -> bool {
        if self.menu.page == Some(crate::menu::Page::Themes)
            || self.theme_save_in_flight()
            // Settings applies its own draft to every window.
            || crate::settings_window::theme_pending(cx)
        {
            return false;
        }
        let light = crate::app::light_appearance(cx);
        let config = self.config.clone();
        let load = cx
            .background_executor()
            .spawn(async move { config.theme(light).map(|theme| (config, theme)) });
        cx.spawn(async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                // Unless a load for a newer appearance is on its way, Herdr
                // now hears this one, with whatever theme this load leaves.
                if light == crate::app::light_appearance(cx) && this.theme_light != light {
                    this.theme_light = light;
                    cx.notify();
                }
                let (config, theme) = match result {
                    Ok(loaded) => loaded,
                    Err(error) => {
                        tracing::warn!(%error, "Could not reload the theme");
                        this.local_error = Some(fl!(
                            "settings-panel-reload-theme-failed",
                            error = error.localize()
                        ));
                        cx.notify();
                        return;
                    }
                };
                if config.theme != this.config.theme
                    || config.contrast != this.config.contrast
                    || light != crate::app::light_appearance(cx)
                    || this.menu.page == Some(crate::menu::Page::Themes)
                    || this.theme_save_in_flight()
                    || crate::settings_window::theme_pending(cx)
                {
                    return;
                }
                this.theme = theme;
                if let Some(mut appearance) =
                    cx.try_global::<crate::app::InitialAppearance>().cloned()
                    && appearance.config.theme == this.config.theme
                {
                    appearance.theme = this.theme.clone();
                    cx.set_global(appearance);
                }
                crate::log_window::set_appearance(&this.config, &this.theme, cx);
                cx.notify();
            });
        })
        .detach();
        true
    }
}

#[cfg(test)]
mod tests;
