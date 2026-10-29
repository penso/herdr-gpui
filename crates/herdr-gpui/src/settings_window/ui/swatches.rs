//! Host colour row: a curated swatch strip, a "none" swatch, a hex field and
//! a reset glyph. Pure rendering; the page owns the colour and the saves.
// i18n: migrated
use crate::{
    config::{FontConfig, Theme, corners, parse_hex},
    fonts::StyledFont,
    i18n::fl,
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use std::rc::Rc;

/// Curated host tints, muted enough to sit behind sidebar text.
pub(in crate::settings_window) const SWATCHES: [u32; 11] = [
    0x3a4a6b, 0x2f5d5a, 0x4a5b2f, 0x6b5a2f, 0x6b3a3a, 0x5a3a6b, 0x2f4a6b, 0x4a4a4a, 0x7a4f2a,
    0x2a6b4f, 0x6b2a5a,
];

const SWATCH_SIZE: f32 = 18.;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::settings_window) enum HostColorChange {
    Set(u32),
    Clear,
}

/// `#rgb` or `#rrggbb` with or without the `#`, any case, padded by spaces.
pub(in crate::settings_window) fn parse_hex_input(text: &str) -> Option<u32> {
    let text = text.trim().to_lowercase();
    if text.starts_with('#') {
        parse_hex(&text)
    } else {
        parse_hex(&format!("#{text}"))
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::settings_window) fn swatch_row(
    theme: &Theme,
    host: &str,
    index: usize,
    current: Option<u32>,
    mono: &FontConfig,
    hint: SharedString,
    hex_input: Option<(Entity<SearchInput>, bool)>,
    on_change: impl Fn(HostColorChange, &mut Window, &mut App) + 'static,
    on_edit_hex: impl Fn(&mut Window, &mut App) + 'static,
) -> Div {
    let on_change = Rc::new(on_change);
    let none = {
        let on_change = on_change.clone();
        div()
            .id(SharedString::from(format!("host-colour-{index}-none")))
            .debug_selector(move || format!("host-colour-{index}-none"))
            .size(px(SWATCH_SIZE))
            .rounded(px(corners::SMALL))
            .border_1()
            .border_dashed()
            .border_color(rgb(if current.is_none() {
                theme.foreground
            } else {
                theme.muted
            }))
            .cursor_pointer()
            .on_click(move |_, window, cx| on_change(HostColorChange::Clear, window, cx))
    };
    let swatches = SWATCHES.iter().enumerate().map(|(k, &color)| {
        let on_change = on_change.clone();
        let selected = current == Some(color);
        div()
            .id(SharedString::from(format!(
                "host-colour-{index}-swatch-{k}"
            )))
            .debug_selector(move || format!("host-colour-{index}-swatch-{k}"))
            .size(px(SWATCH_SIZE))
            .rounded(px(corners::SMALL))
            .bg(rgb(color))
            .border_2()
            .border_color(if selected {
                rgb(theme.foreground).into()
            } else {
                transparent_black()
            })
            .cursor_pointer()
            .on_click(move |_, window, cx| on_change(HostColorChange::Set(color), window, cx))
    });
    let dot = div()
        .size(px(10.))
        .flex_none()
        .rounded_full()
        .map(|dot| match current {
            Some(color) => dot.bg(rgb(color)),
            None => dot
                .border_1()
                .border_dashed()
                .border_color(rgb(theme.muted)),
        });
    let hex = div()
        .id(SharedString::from(format!("host-colour-{index}-hex")))
        .debug_selector(move || format!("host-colour-{index}-hex"))
        .w(px(92.))
        .flex_none()
        .text_size(px(12.))
        .map(|hex| match hex_input {
            // A typed colour that does not parse keeps the field, outlined.
            Some((input, invalid)) => hex
                .rounded(px(corners::SMALL))
                .border_1()
                .border_color(rgb(if invalid {
                    theme.ink(0xd45b5b)
                } else {
                    theme.active
                }))
                .child(input),
            None => {
                let (label, color) = match current {
                    Some(color) => (format!("#{color:06x}"), theme.foreground),
                    None => (fl!("settings-swatch-none"), theme.muted),
                };
                hex.text_font(mono)
                    .text_color(rgb(color))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| on_edit_hex(window, cx))
                    .child(label)
            }
        });
    let reset = div().w(px(20.)).flex_none().map(|slot| {
        if current.is_none() {
            return slot;
        }
        let on_change = on_change.clone();
        slot.child(
            div()
                .id(SharedString::from(format!("host-colour-{index}-reset")))
                .debug_selector(move || format!("host-colour-{index}-reset"))
                .text_color(rgb(theme.muted))
                .cursor_pointer()
                .on_click(move |_, window, cx| on_change(HostColorChange::Clear, window, cx))
                .child("⟲"),
        )
    });
    div()
        .debug_selector(move || format!("host-colour-{index}"))
        .flex()
        .items_center()
        .gap(px(10.))
        .py(px(6.))
        .text_size(px(13.))
        .text_color(rgb(theme.foreground))
        .child(
            div()
                .debug_selector(move || format!("host-colour-{index}-label"))
                .w(px(120.))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(dot)
                .child(div().min_w_0().truncate().child(host.to_owned())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(5.))
                        .child(none)
                        .children(swatches),
                )
                .when(!hint.is_empty(), |col| {
                    col.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme.muted))
                            .child(hint),
                    )
                }),
        )
        .child(hex)
        .child(reset)
}

#[cfg(test)]
mod tests;
