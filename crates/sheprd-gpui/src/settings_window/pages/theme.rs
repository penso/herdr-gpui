//! Theme: appearance mode, the library to browse, the card grid whose hover
//! previews the theme in the real Herdr windows, then the window backdrop and
//! terminal padding.
// i18n: migrated
use super::{SettingsWindow, on_value, ui};
use crate::{
    config::{Config, ThemeName},
    contrast::{Contrast, luminance},
    i18n::fl,
    settings_window::themes::{Choice, FOLLOW, Scope},
};
use gpui::{prelude::*, *};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Light,
    Dark,
    System,
}

#[derive(Clone, Copy, PartialEq)]
enum Library {
    All,
    Ghostty,
    Herdr,
}

/// How the page shows a theme value: [`FOLLOW`] is a config value, shown
/// as a message; every other name is data.
pub(in crate::settings_window) fn display_name(name: &str) -> String {
    if name == FOLLOW {
        fl!("settings-theme-follow-herdr")
    } else {
        name.to_owned()
    }
}

/// The saved theme as a file on disk, when it is one.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn theme_file(name: &str) -> Option<std::path::PathBuf> {
    if let Some(relative) = name.strip_prefix("~/") {
        return Some(std::path::PathBuf::from(std::env::var_os("HOME")?).join(relative));
    }
    std::path::Path::new(name)
        .is_absolute()
        .then(|| std::path::PathBuf::from(name))
}

impl SettingsWindow {
    pub(in crate::settings_window) fn render_theme(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let mode_row = self.theme_mode_row(cx);
        let side_row = self.theme_side_row(cx);
        let follow_row = self.theme_follow_row(cx);
        let contrast_row = self.theme_contrast_row(cx);
        let source_row = self.theme_source_row(cx);
        let search = self.render_theme_search(cx);
        let open_file = self.theme_open_file(cx);
        let results = self.render_theme_results(window, cx);
        let window_rows = self.render_window_rows(cx);
        let theme = &self.theme;
        div()
            .debug_selector(|| "settings-appearance".into())
            .w_full()
            .flex()
            .flex_col()
            .min_w_0()
            .child(ui::section(
                theme,
                &fl!("settings-theme-section-mode"),
                None,
            ))
            .child(mode_row)
            .children(side_row)
            .child(follow_row)
            .child(contrast_row)
            .child(ui::section(
                theme,
                &fl!("settings-theme-section-themes"),
                None,
            ))
            .child(source_row)
            .child(div().mt(px(8.)).child(search))
            .children(open_file)
            .child(
                div()
                    .debug_selector(|| "settings-theme-grid".into())
                    .mt(px(8.))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(results),
            )
            .child(window_rows)
    }

    /// Whether the theme in effect is light, read from the drafted choice
    /// when its colors are at hand.
    pub(in crate::settings_window) fn drafted_is_light(&self, cx: &App) -> bool {
        let light = crate::app::light_appearance(cx);
        let background = self
            .prepared_app_theme(self.drafted_theme(), light)
            .map_or(self.theme.background, |theme| theme.background);
        luminance(background) > 0.4
    }

    fn theme_mode_row(&self, cx: &mut Context<Self>) -> ui::Row {
        let following = ThemeName::follows_system(self.drafted_theme());
        let is_light = self.drafted_is_light(cx);
        let selected = if following {
            Mode::System
        } else if is_light {
            Mode::Light
        } else {
            Mode::Dark
        };
        ui::row(&self.theme, "theme-mode", fl!("settings-theme-mode"))
            .description(fl!("settings-theme-mode-hint"))
            .control(ui::segmented(
                &self.theme,
                "theme-mode-control",
                &[
                    (Mode::Light, fl!("settings-theme-light")),
                    (Mode::Dark, fl!("settings-theme-dark")),
                    (Mode::System, fl!("settings-theme-system")),
                ],
                Some(selected),
                true,
                on_value(cx, move |this, mode, window, cx| {
                    let focus = this.themes.search.read(cx).focus.clone();
                    window.focus(&focus, cx);
                    match mode {
                        Mode::System => this.start_system_theme(cx),
                        Mode::Light => this.use_single_theme(true, cx),
                        Mode::Dark => this.use_single_theme(false, cx),
                    }
                }),
            ))
    }

    /// Shown while following: which side of the pair the grid changes.
    fn theme_side_row(&self, cx: &mut Context<Self>) -> Option<ui::Row> {
        let value = self.drafted_theme();
        ThemeName::follows_system(value).then(|| {
            ui::row(&self.theme, "theme-side", fl!("settings-theme-side"))
                .description(fl!(
                    "settings-theme-side-hint",
                    light = display_name(ThemeName::side(value, true)),
                    dark = display_name(ThemeName::side(value, false)),
                ))
                .control(ui::segmented(
                    &self.theme,
                    "theme-side-control",
                    &[
                        (true, fl!("settings-theme-light")),
                        (false, fl!("settings-theme-dark")),
                    ],
                    Some(self.themes.editing_light),
                    true,
                    on_value(cx, |this, light, window, cx| {
                        let focus = this.themes.search.read(cx).focus.clone();
                        window.focus(&focus, cx);
                        this.edit_theme_side(light, cx);
                    }),
                ))
        })
    }

    fn theme_follow_row(&self, cx: &mut Context<Self>) -> ui::Row {
        let following = self.config.theme == FOLLOW;
        ui::row(&self.theme, "theme-follow", fl!("settings-theme-follow"))
            .tag(ui::Source::Herdr)
            .description(fl!("settings-theme-follow-description"))
            .switch(
                ui::toggle(&self.theme, "theme-follow-toggle", following, true).on_click(
                    cx.listener(move |this, _, _, cx| {
                        let name = if following {
                            this.themes
                                .choice()
                                .filter(|choice| choice.scope == Scope::App)
                                .map_or_else(|| "Default".to_owned(), |choice| choice.name) // i18n: data
                        } else {
                            FOLLOW.to_owned()
                        };
                        this.accept_theme_choice(
                            Choice {
                                scope: Scope::App,
                                name,
                            },
                            cx,
                        );
                    }),
                ),
            )
    }

    fn theme_contrast_row(&self, cx: &mut Context<Self>) -> ui::Row {
        let high = self.config.contrast == Contrast::High;
        ui::row(
            &self.theme,
            "theme-contrast",
            fl!("settings-theme-contrast"),
        )
        .description(fl!("settings-theme-contrast-description"))
        .switch(
            ui::toggle(&self.theme, "theme-contrast-toggle", high, true).on_click(cx.listener(
                move |this, _, _, cx| {
                    let contrast = if high {
                        Contrast::Standard
                    } else {
                        Contrast::High
                    };
                    this.save_native(move || Config::save_contrast(contrast), cx);
                },
            )),
        )
    }

    fn theme_source_row(&self, cx: &mut Context<Self>) -> ui::Row {
        let selected = match (self.themes.ghostty_enabled, self.themes.herdr_enabled) {
            (true, true) => Library::All,
            (true, false) => Library::Ghostty,
            (false, _) => Library::Herdr,
        };
        let note = if Scope::Herdr.editable() {
            fl!("settings-theme-source-description")
        } else {
            fl!("settings-theme-library-read-only")
        };
        ui::row(&self.theme, "theme-source", fl!("settings-theme-source"))
            .description(note)
            .control(ui::segmented(
                &self.theme,
                "theme-source-control",
                &[
                    (Library::All, fl!("settings-theme-library-all")),
                    (Library::Ghostty, fl!("settings-theme-library-ghostty")),
                    (Library::Herdr, fl!("settings-theme-library-herdr")),
                ],
                Some(selected),
                true,
                on_value(cx, |this, library, window, cx| {
                    let focus = this.themes.search.read(cx).focus.clone();
                    window.focus(&focus, cx);
                    let (ghostty, herdr) = match library {
                        Library::All => (true, true),
                        Library::Ghostty => (true, false),
                        Library::Herdr => (false, true),
                    };
                    this.set_theme_sources(ghostty, herdr, cx);
                }),
            ))
    }

    /// A button that opens the saved theme when it is a file.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn theme_open_file(&self, cx: &mut Context<Self>) -> Option<Div> {
        let path = theme_file(self.edited_theme())?;
        Some(
            div().mt(px(8.)).flex().child(
                ui::button(
                    &self.theme,
                    "theme-open-file",
                    fl!("settings-theme-open-file"),
                    true,
                )
                .on_click(
                    cx.listener(move |_, _, _, cx| super::open_path(false, path.clone(), cx)),
                ),
            ),
        )
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn theme_open_file(&self, _: &mut Context<Self>) -> Option<Div> {
        None
    }
}
