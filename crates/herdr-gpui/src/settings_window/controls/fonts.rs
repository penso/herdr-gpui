//! Compact per-role controls and a root-level, transient family chooser.
// i18n: migrated
use super::*;
use crate::{
    config::FontConfig,
    fonts::StyledFont,
    i18n::{self, fl},
};

/// The terminal face's specimen: code, the same in every language.
const TERMINAL_SAMPLE: &str = "let answer = 42;\nif answer != 0 {\n    println!(\"ready\");\n}"; // i18n: data

#[cfg(all(feature = "integration-test", target_os = "macos"))]
#[derive(Default)]
struct NativeBounds(std::collections::HashMap<String, Bounds<Pixels>>);
#[cfg(all(feature = "integration-test", target_os = "macos"))]
impl Global for NativeBounds {}

#[cfg(all(feature = "integration-test", target_os = "macos"))]
pub(in crate::settings_window) fn native_probe(id: impl Into<String>) -> impl IntoElement {
    let id = id.into();
    canvas(
        |_, _, _| (),
        move |bounds, _, _, cx| {
            cx.default_global::<NativeBounds>()
                .0
                .insert(id.clone(), bounds);
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(all(feature = "integration-test", target_os = "macos"))]
impl SettingsWindow {
    pub(in crate::settings_window) fn native_font_bounds(
        &self,
        id: &str,
        cx: &App,
    ) -> Option<Bounds<Pixels>> {
        cx.try_global::<NativeBounds>()?.0.get(id).copied()
    }

    pub(in crate::settings_window) fn native_font_search(
        &self,
        cx: &App,
    ) -> (FocusHandle, String, usize, usize, bool) {
        let input = self.controls.search.read(cx);
        (
            input.focus.clone(),
            input.text().to_owned(),
            self.controls.filtered.len(),
            self.controls.names.len(),
            self.controls.discovering,
        )
    }

    pub(in crate::settings_window) fn native_font_selection(
        &self,
    ) -> (Option<FontTarget>, FontFace, FontConfig) {
        (
            self.controls.picker,
            self.controls.active_face,
            self.control_specimen_font(),
        )
    }

    pub(in crate::settings_window) fn native_font_editor(
        &self,
        cx: &App,
    ) -> Option<(FocusHandle, String)> {
        let input = self.number_edit.as_ref()?.input.read(cx);
        Some((input.focus.clone(), input.text().to_owned()))
    }

    pub(in crate::settings_window) fn native_font_sizes_idle(&self) -> bool {
        self.controls.pending_sizes.is_empty() && self.controls.saving_sizes.is_empty()
    }
}

#[cfg(test)]
type FamilyWriter = dyn Fn(FontTarget, Option<String>) -> crate::Result<()> + Send + Sync;

#[cfg(test)]
#[derive(Clone)]
pub(super) struct FamilyIo {
    write: std::sync::Arc<FamilyWriter>,
    load: std::sync::Arc<dyn Fn() -> crate::Result<super::super::Loaded> + Send + Sync>,
}

pub(in crate::settings_window) fn face_font(config: &Config, face: FontFace) -> &FontConfig {
    match face {
        FontFace::Terminal => &config.terminal,
        FontFace::Sidebar => &config.sidebar,
        FontFace::Tabs => &config.tabs,
        FontFace::Ui => &config.ui,
    }
}

/// How a family is shown: its name, which is data, except the platform's
/// interface font.
pub(in crate::settings_window) fn family_label(family: &str) -> String {
    // i18n: data
    if family == ".SystemUIFont" {
        fl!("settings-fonts-system-font")
    } else {
        family.to_owned()
    }
}

/// The message titling a face, its Settings row's.
pub(in crate::settings_window) const fn face_message(face: FontFace) -> &'static str {
    match face {
        FontFace::Terminal => "settings-fonts-terminal",
        FontFace::Sidebar => "settings-fonts-sidebar",
        FontFace::Tabs => "settings-fonts-tabs",
        FontFace::Ui => "settings-fonts-ui",
    }
}

/// The messages of the specimen's lines for a face. The terminal's is code,
/// the same in every language.
pub(in crate::settings_window) const fn sample_messages(face: FontFace) -> &'static [&'static str] {
    match face {
        FontFace::Terminal => &[],
        FontFace::Sidebar => &[
            "settings-fonts-sample-sidebar-1",
            "settings-fonts-sample-sidebar-2",
            "settings-fonts-sample-sidebar-3",
        ],
        FontFace::Tabs => &[
            "settings-fonts-sample-tabs-1",
            "settings-fonts-sample-tabs-2",
        ],
        FontFace::Ui => &["settings-fonts-sample-ui-1", "settings-fonts-sample-ui-2"],
    }
}

/// Which face the picker sets, as its messages select it.
fn role(target: FontTarget) -> &'static str {
    match target {
        FontTarget::All => "all",
        FontTarget::Face(face) => face.name(),
    }
}

impl SettingsWindow {
    pub(in crate::settings_window) fn open_control_font_picker(
        &mut self,
        target: FontTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.finish_number_edit(true, cx) {
            self.finish_number_edit(false, cx);
        }
        if let FontTarget::Face(face) = target {
            self.controls.active_face = face;
        }
        self.controls.picker = Some(target);
        self.controls.search.update(cx, |input, cx| input.clear(cx));
        self.controls.filtered = filter_fonts(&self.controls.names, "");
        self.controls.selected = 0;
        self.controls.scroll.scroll_to_item(0, ScrollStrategy::Top);
        window.focus(&self.controls.search.read(cx).focus.clone(), cx);
        cx.notify();
    }

    pub(in crate::settings_window) fn dismiss_control_font_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.controls.picker.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    fn commit_control_family(
        &mut self,
        target: FontTarget,
        family: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Rendered callbacks own their target, never a later row's mutable selection.
        if self.busy() || self.controls.picker != Some(target) {
            return;
        }
        self.dismiss_control_font_picker(window, cx);
        #[cfg(test)]
        if let Some(io) = self.controls.family_io.clone() {
            self.save_with(
                move || (io.write)(target, family),
                move || (io.load)(),
                false,
                cx,
            );
            return;
        }
        self.save_native(
            move || match target {
                FontTarget::All => Config::save_all_font_families(family.as_deref()),
                FontTarget::Face(face) => Config::save_font_family(face, family.as_deref()),
            },
            cx,
        );
    }

    fn control_font_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.controls.picker else {
            return;
        };
        let key = event.keystroke.key.as_str();
        if !matches!(key, "up" | "down" | "enter" | "escape") {
            return;
        }
        cx.stop_propagation();
        if self.controls.search.read(cx).is_composing() {
            return;
        }
        window.prevent_default();
        match key {
            "escape" => self.dismiss_control_font_picker(window, cx),
            "enter" => {
                if let Some(index) = self.controls.filtered.get(self.controls.selected) {
                    self.commit_control_family(
                        target,
                        Some(self.controls.names[*index].clone()),
                        window,
                        cx,
                    );
                }
            }
            _ => {
                self.controls.selected = if key == "up" {
                    self.controls.selected.saturating_sub(1)
                } else {
                    (self.controls.selected + 1).min(self.controls.filtered.len().saturating_sub(1))
                };
                self.controls
                    .scroll
                    .scroll_to_item(self.controls.selected, ScrollStrategy::Top);
                cx.notify();
            }
        }
    }

    pub(in crate::settings_window) fn font_button(
        &self,
        id: String,
        label: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let selector = id.clone();
        #[cfg(all(feature = "integration-test", target_os = "macos"))]
        let probe = native_probe(id.clone());
        div()
            .id(id)
            .debug_selector(move || selector.clone())
            .flex()
            .items_center()
            .justify_center()
            .h(px(28.))
            .px(px(8.))
            .min_w_0()
            .rounded(px(corners::CONTROL))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.theme.active)))
            .child(label.into())
            .map(|button| {
                #[cfg(all(feature = "integration-test", target_os = "macos"))]
                let button = button.child(probe);
                button
            })
    }

    pub(in crate::settings_window) fn control_specimen_font(&self) -> FontConfig {
        let face = self.controls.active_face;
        let mut font = face_font(&self.config, face).clone();
        font.size = self.controls.size(face, &self.config);
        font
    }

    /// The size shown for a face: a queued or saving edit wins over the file.
    pub(in crate::settings_window) fn control_face_size(&self, face: FontFace) -> f32 {
        self.controls.size(face, &self.config)
    }

    pub(in crate::settings_window) fn active_font_face(&self) -> FontFace {
        self.controls.active_face
    }

    /// Forgets last frame's painted bounds so the native checks read fresh ones.
    #[cfg(all(feature = "integration-test", target_os = "macos"))]
    pub(in crate::settings_window) fn reset_native_font_bounds(&self, cx: &mut Context<Self>) {
        cx.default_global::<NativeBounds>().0.clear();
    }

    /// The live sample for the active face, set in its own font and size.
    pub(in crate::settings_window) fn render_font_specimen(&self) -> Div {
        let font = self.control_specimen_font();
        let sample = match self.controls.active_face {
            FontFace::Terminal => TERMINAL_SAMPLE.to_owned(),
            face => sample_messages(face)
                .iter()
                .map(|line| i18n::message(line, &[]))
                .collect::<Vec<_>>()
                .join("\n"),
        };
        div()
            .debug_selector(|| "settings-font-specimen".into())
            .map(|specimen| {
                #[cfg(all(feature = "integration-test", target_os = "macos"))]
                let specimen = specimen.child(native_probe("settings-font-specimen"));
                specimen
            })
            .mt(px(12.))
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap(px(20.))
            .p(px(20.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(self.theme.surface))
            .child(self.control_note(fl!("settings-fonts-size", size = font.size)))
            .child(
                div()
                    .text_font(&font)
                    .text_size(px(font.size))
                    .line_height(px(font.line_height()))
                    .child(sample),
            )
            .child(
                div()
                    .text_font(&font)
                    .text_size(px(font.size))
                    .line_height(px(font.line_height()))
                    .child("0O 1lI {} [] != <="),
            )
    }

    pub(in crate::settings_window) fn render_control_font_picker(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if self.section != Section::Fonts {
            return None;
        }
        let target = self.controls.picker?;
        let role = role(target);
        let list_height = (f32::from(window.viewport_size().height) - 340.).clamp(84., 336.);
        Some(
            div()
                .absolute()
                .top(px(36.))
                .bottom(px(36.))
                .left(px(184.))
                .right_0()
                .occlude()
                .bg(rgb(self.theme.background).opacity(0.75))
                .child(
                    div()
                        .id("settings-font-backdrop")
                        .absolute()
                        .inset_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_control_font_picker(window, cx);
                            }),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(64.))
                        .left(px(20.))
                        .right(px(20.))
                        .occlude()
                        .id("settings-font-picker")
                        .debug_selector(|| "settings-font-picker".into())
                        .map(|picker| {
                            #[cfg(all(feature = "integration-test", target_os = "macos"))]
                            let picker = picker.child(native_probe("settings-font-picker"));
                            picker
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_key_down(cx.listener(Self::control_font_key))
                        .p(px(16.))
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .gap(px(12.))
                        .rounded(px(corners::PANEL))
                        .border_1()
                        .border_color(rgb(self.theme.active))
                        .bg(rgb(self.theme.surface))
                        .shadow_lg()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(fl!("settings-fonts-picker-title", role = role))
                                .child(
                                    self.font_button(
                                        "settings-font-picker-close".into(),
                                        fl!("settings-fonts-picker-close"),
                                    )
                                    .on_click(cx.listener(
                                        |this, _, window, cx| {
                                            this.dismiss_control_font_picker(window, cx)
                                        },
                                    )),
                                ),
                        )
                        .child(
                            div()
                                .debug_selector(|| "settings-font-search".into())
                                .map(|search| {
                                    #[cfg(all(feature = "integration-test", target_os = "macos"))]
                                    let search = search.child(native_probe("settings-font-search"));
                                    search
                                })
                                .child(self.controls.search.clone()),
                        )
                        .child(self.control_note(if self.controls.discovering {
                            fl!("settings-fonts-loading")
                        } else {
                            fl!(
                                "settings-fonts-count",
                                shown = self.controls.filtered.len(),
                                total = self.controls.names.len(),
                            )
                        }))
                        .child(
                            uniform_list(
                                "settings-font-results",
                                self.controls.filtered.len(),
                                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                    range
                                        .map(|index| {
                                            let family = this.controls.names
                                                [this.controls.filtered[index]]
                                                .clone();
                                            this.font_button(
                                                format!("settings-font-result-{index}"),
                                                "",
                                            )
                                            .w_full()
                                            .h(px(28.))
                                            .justify_start()
                                            .overflow_hidden()
                                            .when(index == this.controls.selected, |row| {
                                                row.bg(rgb(this.theme.active))
                                            })
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(family_label(&family)),
                                            )
                                            .on_click(
                                                cx.listener(move |this, _, window, cx| {
                                                    this.commit_control_family(
                                                        target,
                                                        Some(family.clone()),
                                                        window,
                                                        cx,
                                                    );
                                                }),
                                            )
                                        })
                                        .collect()
                                }),
                            )
                            .debug_selector(|| "settings-font-results".into())
                            .h(px(list_height))
                            .track_scroll(&self.controls.scroll)
                            .map(|list| {
                                #[cfg(all(feature = "integration-test", target_os = "macos"))]
                                let list = div()
                                    .h(px(list_height))
                                    .child(native_probe("settings-font-results"))
                                    .child(list);
                                list
                            }),
                        )
                        .when(
                            !self.controls.discovering && self.controls.filtered.is_empty(),
                            |panel| panel.child(self.control_note(fl!("settings-fonts-no-match"))),
                        )
                        .child(
                            self.font_button(
                                "settings-font-default".into(),
                                fl!("settings-fonts-picker-default", role = role),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.commit_control_family(target, None, window, cx)
                                },
                            )),
                        ),
                ),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
