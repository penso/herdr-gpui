//! The Azure DevOps page from the keyboard: Tab and Shift-Tab move between
//! its two fields, Enter checks and saves the token typed, and Escape closes
//! the menu. Every other key types into the field that has focus.

use crate::HerdrWindow;
use gpui::{prelude::*, *};

/// What a key does on the Azure DevOps page, apart from typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AzureKey {
    Dismiss,
    Save,
    /// Tab or Shift-Tab: the other field.
    Move,
}

impl AzureKey {
    pub(crate) fn of(keystroke: &Keystroke) -> Option<Self> {
        let modifiers = keystroke.modifiers;
        let plain = !modifiers.control && !modifiers.platform && !modifiers.alt;
        Some(match keystroke.key.as_str() {
            "escape" => Self::Dismiss,
            "enter" if modifiers == Modifiers::default() => Self::Save,
            "tab" if plain => Self::Move,
            _ => return None,
        })
    }
}

impl HerdrWindow {
    /// Keys while the Azure DevOps page shows.
    pub(in crate::menu) fn azure_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = AzureKey::of(&event.keystroke) else {
            return;
        };
        let azure = &self.menu.azure;
        let composing = [&azure.organization, &azure.token]
            .into_iter()
            .flatten()
            .any(|input| input.read(cx).is_composing());
        if composing {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        match key {
            AzureKey::Dismiss => self.dismiss_menu(window, cx),
            AzureKey::Save => self.save_azure(cx),
            AzureKey::Move => {
                let on_organization = azure
                    .organization
                    .as_ref()
                    .is_some_and(|input| input.read(cx).focus.is_focused(window));
                let next = if on_organization {
                    &azure.token
                } else {
                    &azure.organization
                };
                if let Some(input) = next {
                    let focus = input.read(cx).focus.clone();
                    window.focus(&focus, cx);
                }
            }
        }
        cx.notify();
    }
}
