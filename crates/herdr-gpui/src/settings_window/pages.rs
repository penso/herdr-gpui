//! The behaviour pages: one module per Settings page, each rebuilt from the
//! shared row and control builders in `ui`.
mod configuration;
mod fonts;
mod general;
mod indicators;
pub(in crate::settings_window) mod notifications;
pub(in crate::settings_window) mod remote_control;
mod sidebar;
mod status_bar;
mod tabs;
mod theme;
mod window;

use super::{SettingsWindow, inputs::DropdownField, ui};
use crate::i18n::{self, fl};
pub(super) use fonts::size_control;
use gpui::{prelude::*, *};
pub(super) use sidebar::Spacing;
pub(super) use window::padding_ids;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(in crate::settings_window) use configuration::open_path;

/// The text on an override dropdown: the local choice, or what Herdr says.
pub(super) fn override_label(shared_value: &str, local: Option<&str>) -> String {
    match local {
        Some(choice) => choice.to_owned(),
        None => fl!("settings-use-herdr", value = shared_value),
    }
}

impl SettingsWindow {
    /// Disables a row that edits Herdr's shared file when that file cannot be
    /// written, saying why instead of showing the description.
    fn gate_shared(&self, row: ui::Row) -> ui::Row {
        match self.shared_gate() {
            Some(reason) => row.disabled(reason),
            None => row,
        }
    }

    /// Why rows that edit Herdr's shared file are disabled, if they are.
    pub(in crate::settings_window) fn shared_gate(&self) -> Option<String> {
        if !cfg!(unix) {
            Some(fl!("settings-shared-read-only"))
        } else if self.shared.is_none() {
            Some(fl!("settings-shared-unavailable"))
        } else {
            None
        }
    }

    /// A dropdown whose first entry follows Herdr and whose other entries are
    /// local overrides. `local` is the index into `choices` of the current one.
    fn override_dropdown(
        &self,
        field: DropdownField,
        shared_value: &str,
        choices: &[String],
        local: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = override_label(
            shared_value,
            local.and_then(|i| choices.get(i)).map(String::as_str),
        );
        let options: Vec<SharedString> = std::iter::once(override_label(shared_value, None))
            .chain(choices.iter().cloned())
            .map(SharedString::from)
            .collect();
        let selected = local.map_or(0, |index| index + 1);
        let view = cx.entity().downgrade();
        let (opener, picker, dismisser) = (view.clone(), view.clone(), view);
        ui::dropdown(
            &self.theme,
            field.selector(),
            label,
            true,
            true,
            self.dropdown.as_ref().filter(|open| open.field == field),
            move |window, cx| {
                let options = options.clone();
                let _ = opener.update(cx, |this, cx| {
                    this.open_dropdown(field, options, selected, window, cx)
                });
            },
            move |index, window, cx| {
                let _ = picker.update(cx, |this, cx| {
                    this.pick_dropdown(field, index, cx);
                    window.focus(&this.focus, cx);
                });
            },
            move |window, cx| {
                let _ = dismisser.update(cx, |this, cx| this.close_dropdown(window, cx));
            },
        )
    }
}

/// A callback for a control that reports a value rather than a click.
fn on_value<T: 'static>(
    cx: &Context<SettingsWindow>,
    act: impl Fn(&mut SettingsWindow, T, &mut Window, &mut Context<SettingsWindow>) + 'static,
) -> impl Fn(T, &mut Window, &mut App) + 'static {
    let view = cx.entity().downgrade();
    move |value, window, cx| {
        let _ = view.update(cx, |this, cx| act(this, value, window, cx));
    }
}

/// The clipboard notification's positions, each with its message.
pub(super) const CLIPBOARD_POSITIONS: [(crate::config::ClipboardToastPosition, &str); 6] = {
    use crate::config::ClipboardToastPosition as P;
    [
        (P::TopLeft, "settings-position-top-left"),
        (P::TopCenter, "settings-position-top-center"),
        (P::TopRight, "settings-position-top-right"),
        (P::BottomLeft, "settings-position-bottom-left"),
        (P::BottomCenter, "settings-position-bottom-center"),
        (P::BottomRight, "settings-position-bottom-right"),
    ]
};

/// Herdr's toast positions, each with its message.
pub(super) const TOAST_POSITIONS: [(herdr_client::protocol::ToastHerdrPosition, &str); 4] = {
    use herdr_client::protocol::ToastHerdrPosition as P;
    [
        (P::TopLeft, "settings-position-top-left"),
        (P::TopRight, "settings-position-top-right"),
        (P::BottomLeft, "settings-position-bottom-left"),
        (P::BottomRight, "settings-position-bottom-right"),
    ]
};

/// The names of `positions`, in order, for an override dropdown.
fn position_names<T>(positions: &[(T, &str)]) -> Vec<String> {
    positions
        .iter()
        .map(|(_, id)| i18n::message(id, &[]))
        .collect()
}

/// The name of `position` among `positions`, or nothing when it is missing.
fn position_name<T: PartialEq>(positions: &[(T, &str)], position: &T) -> String {
    positions
        .iter()
        .find(|(p, _)| p == position)
        .map(|(_, id)| i18n::message(id, &[]))
        .unwrap_or_default()
}

/// "On" or "Off" for a boolean override dropdown.
fn on_off(value: bool) -> String {
    if value {
        fl!("settings-on")
    } else {
        fl!("settings-off")
    }
}

/// The choices of a boolean override dropdown, on first.
fn on_off_choices() -> [String; 2] {
    [on_off(true), on_off(false)]
}
