//! Settings > Theme > Window backdrop: Solid, Translucent, or Picture,
//! each with only its own controls below. Settings and dialogs stay opaque
//! whatever is chosen.
// i18n: migrated
use super::*;
use crate::config::{
    Backdrop, Material, Theme,
    backdrop::{Edit, IMAGE_OPACITY},
};
use crate::{i18n::fl, settings_window::ui};

mod ruler;

pub(in crate::settings_window) use ruler::Ruler;
#[cfg(test)]
pub(in crate::settings_window) use ruler::{fraction, opacity_at};

/// The opacity Translucent starts at from a solid window.
const TRANSLUCENT_OPACITY: f32 = 0.85;
/// What the window shows behind its background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::settings_window) enum Mode {
    Solid,
    Translucent,
    Picture,
}

impl Mode {
    pub(in crate::settings_window) fn of(backdrop: &Backdrop) -> Self {
        match (&backdrop.image, backdrop.translucent()) {
            (Some(_), _) => Self::Picture,
            (None, true) => Self::Translucent,
            (None, false) => Self::Solid,
        }
    }

    fn name(self) -> String {
        match self {
            Self::Solid => fl!("settings-theme-backdrop-solid"),
            Self::Translucent => fl!("settings-theme-backdrop-translucent"),
            Self::Picture => fl!("settings-theme-backdrop-picture"),
        }
    }

    /// The line under the Backdrop row while this mode is chosen.
    fn description(self) -> String {
        match self {
            Self::Solid => fl!("settings-theme-backdrop-solid-hint"),
            Self::Translucent => fl!("settings-theme-backdrop-translucent-hint"),
            Self::Picture => fl!("settings-theme-backdrop-picture-hint"),
        }
    }

    fn selector(self) -> &'static str {
        match self {
            Self::Solid => "settings-backdrop-solid",
            Self::Translucent => "settings-backdrop-translucent",
            Self::Picture => "settings-backdrop-picture",
        }
    }

    /// The edits that switch `backdrop` to this mode. Picture has none: it
    /// needs a file, which the chooser supplies.
    pub(in crate::settings_window) fn edits(self, backdrop: &Backdrop) -> Vec<Edit> {
        let image = backdrop.image.is_some().then_some(Edit::Image(None));
        match self {
            Self::Solid => image.into_iter().chain([Edit::Opacity(1.)]).collect(),
            Self::Translucent => image
                .into_iter()
                .chain((!backdrop.translucent()).then_some(Edit::Opacity(TRANSLUCENT_OPACITY)))
                .collect(),
            Self::Picture => Vec::new(),
        }
    }
}

/// A thumbnail-size window: a title strip, a sidebar, and a few terminal
/// lines, its background at `alpha` over what is behind it.
fn miniature(theme: &Theme, alpha: f32) -> Div {
    let fill = |color: u32| Hsla::from(rgb(color)).opacity(alpha);
    let line = |width: f32, color: u32| div().h(px(3.)).w(px(width)).rounded(px(1.)).bg(rgb(color));
    div()
        .w(px(112.))
        .h(px(60.))
        .flex()
        .flex_col()
        .rounded(px(corners::SMALL))
        .overflow_hidden()
        .border_1()
        .border_color(rgb(theme.active))
        .child(div().h(px(8.)).flex_none().bg(fill(theme.surface)))
        .child(
            div()
                .flex()
                .flex_1()
                .child(div().w(px(28.)).h_full().bg(fill(theme.surface)))
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .p(px(5.))
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .bg(fill(theme.background))
                        .child(line(36., theme.palette[2]))
                        .child(line(52., theme.foreground))
                        .child(line(28., theme.foreground))
                        .child(line(44., theme.palette[3])),
                ),
        )
}

/// A stand-in for what shows behind each mode's miniature.
fn behind(theme: &Theme, mode: Mode) -> Div {
    match mode {
        Mode::Solid => div().bg(rgb(theme.background)),
        // Stripes of the theme's hues read as "something behind".
        Mode::Translucent => div()
            .flex()
            .children((1..=6).map(|hue| div().flex_1().h_full().bg(rgb(theme.palette[hue])))),
        Mode::Picture => div().bg(linear_gradient(
            135.,
            linear_color_stop(rgb(theme.palette[4]), 0.),
            linear_color_stop(rgb(theme.palette[5]), 1.),
        )),
    }
}

/// Whether this system has more than one blur to pick: DWM's acrylic and
/// Mica, from Windows 11 22H2.
fn offers_materials() -> bool {
    #[cfg(windows)]
    return crate::caption::windows::system_backdrops();
    #[cfg(not(windows))]
    false
}

impl SettingsWindow {
    pub(in crate::settings_window) fn render_backdrop_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> Div {
        let backdrop = &self.config.backdrop;
        let current = Mode::of(backdrop);
        let mut tiles = div().flex().flex_wrap().gap(px(10.));
        for mode in [Mode::Solid, Mode::Translucent, Mode::Picture] {
            tiles = tiles.child(self.backdrop_tile(mode, current == mode, cx));
        }
        let details = match current {
            Mode::Solid => div(),
            Mode::Translucent => {
                let blur = backdrop.blur;
                div()
                    .flex()
                    .flex_col()
                    .child(self.backdrop_ruler(cx))
                    .child(
                        self.control_switch(
                            "settings-backdrop-blur",
                            fl!("settings-theme-backdrop-blur"),
                            blur,
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.save_backdrop(vec![Edit::Blur(!blur)], cx);
                        })),
                    )
                    // Only Windows 11 has a second material to choose.
                    .when(blur && offers_materials(), |details| {
                        let mica = backdrop.material == Material::Mica;
                        let next = if mica {
                            Material::Acrylic
                        } else {
                            Material::Mica
                        };
                        details
                            .child(
                                self.control_switch(
                                    "settings-backdrop-mica",
                                    fl!("settings-theme-backdrop-mica"),
                                    mica,
                                    true,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.save_backdrop(vec![Edit::Material(next)], cx);
                                    },
                                )),
                            )
                            .child(self.control_note(fl!("settings-theme-backdrop-mica-hint")))
                    })
            }
            Mode::Picture => div()
                .flex()
                .flex_col()
                .child(self.backdrop_picture_row(cx))
                .child(self.backdrop_ruler(cx)),
        };
        div()
            .debug_selector(|| "settings-backdrop".into())
            .flex()
            .flex_col()
            .child(
                ui::row(
                    &self.theme,
                    "theme-backdrop",
                    fl!("settings-theme-backdrop"),
                )
                .description(current.description()),
            )
            .child(tiles.py(px(10.)))
            .child(details)
    }

    fn backdrop_tile(&self, mode: Mode, selected: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        let alpha = match mode {
            Mode::Solid => 1.,
            Mode::Translucent => 0.6,
            Mode::Picture => 0.7,
        };
        let selector = mode.selector();
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .flex_1()
            .min_w(px(132.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(corners::CONTROL))
            .border_2()
            .border_color(if selected {
                crate::menu::accent(theme)
            } else {
                rgb(theme.active)
            })
            .bg(rgb(theme.background))
            .child(
                behind(theme, mode)
                    .relative()
                    .w_full()
                    .h(px(76.))
                    .rounded(px(corners::SMALL))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(miniature(theme, alpha)),
                    ),
            )
            .child(mode.name())
            .when(!selected, |tile| {
                tile.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if mode == Mode::Picture {
                            this.choose_backdrop_picture(cx);
                        } else {
                            let edits = mode.edits(&this.config.backdrop);
                            this.save_backdrop(edits, cx);
                        }
                    }))
            })
    }

    fn backdrop_picture_row(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let name = self
            .config
            .backdrop
            .image
            .as_deref()
            .map(|image| {
                image
                    .file_name()
                    .unwrap_or(image.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default();
        let link = |id: &'static str, label: String, color: u32| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_none()
                .cursor_pointer()
                .text_color(rgb(color))
                .child(label)
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .py(px(8.))
            .child(
                div()
                    .flex_none()
                    .child(fl!("settings-theme-backdrop-picture")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .min_w_0()
                    .child(div().min_w_0().truncate().child(name))
                    .child(
                        link(
                            "settings-backdrop-change",
                            fl!("settings-theme-backdrop-change"),
                            theme.muted,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.choose_backdrop_picture(cx))),
                    )
                    .child(
                        link(
                            "settings-backdrop-remove",
                            fl!("settings-theme-backdrop-remove"),
                            theme.ink(theme.palette[1]),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            // Back to the translucent window it lay on.
                            this.save_backdrop(vec![Edit::Image(None)], cx);
                        })),
                    ),
            )
    }

    fn save_backdrop(&mut self, edits: Vec<Edit>, cx: &mut Context<Self>) {
        if edits.is_empty() {
            return;
        }
        self.save_native(
            move || edits.into_iter().try_for_each(Config::save_backdrop),
            cx,
        );
    }

    fn choose_backdrop_picture(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(fl!("settings-theme-backdrop-choose").into()),
        });
        // An opaque background would hide the picture just chosen.
        let opaque = !self.config.backdrop.translucent();
        cx.spawn(async move |this, cx| {
            let path = match chosen.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Err(error)) => {
                    tracing::warn!(%error, "Could not open the picture chooser");
                    None
                }
                Ok(Ok(None)) | Err(_) => None,
            };
            let Some(path) = path else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.save_native(
                    move || {
                        // Refused here, the error shows in Settings rather
                        // than only in the log once the window tries it.
                        crate::backdrop::decode(&path)?;
                        Config::save_backdrop(Edit::Image(Some(path)))?;
                        if opaque {
                            Config::save_backdrop(Edit::Opacity(IMAGE_OPACITY))?;
                        }
                        Ok(())
                    },
                    cx,
                );
            });
        })
        .detach();
    }
}
