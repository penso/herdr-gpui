//! The card that says why terminal icons draw as boxes: no Nerd Font was
//! found. Fonts are not bundled, so the app names the fix and leaves
//! installing it to the user. The command is shown and copied, never run.
use super::{Flash, HerdrWindow};
use crate::{config_diagnostic::ConfigDiagnostic, fonts::StyledFont};
use gpui::{prelude::*, *};
use herdr_client::protocol::FrameData;

/// Each line is drawn truncated, so it must fit the card on its own. Only
/// macOS reports a missing font, so the fix can name Homebrew.
const NOTICE: &str = "Terminal icons need a Nerd Font; none was found.\n\
                      Install one with Homebrew, then restart Herdr:";

/// Drawn apart from the text, in the terminal face, so it reads as something
/// to run, and copied whole even where a narrow card truncates it.
const COMMAND: &str = "brew install --cask font-symbols-only-nerd-font";

/// The text column starts past the card's accent dot and its gap.
const TEXT_INDENT: f32 = 14.;

#[derive(Debug, Default)]
pub(crate) struct SymbolFontNotice {
    /// A terminal frame has shown an icon cell since this window opened. One
    /// sighting is enough: whatever drew it draws it again.
    icon_seen: bool,
    card: ConfigDiagnostic,
}

impl SymbolFontNotice {
    /// Follows the config and the frame being drawn, returning whether the
    /// card appeared or went away. `frame` is scanned only while a font is
    /// missing and no icon has been seen, so a machine with an icon font, and
    /// one already told, pay nothing per frame.
    fn observe(&mut self, missing: bool, frame: Option<&FrameData>) -> bool {
        let shown = self.card.visible().is_some();
        if missing && !self.icon_seen {
            self.icon_seen = frame.is_some_and(crate::terminal_painter::frame_needs_symbol_font);
        }
        self.card
            .sync((missing && self.icon_seen).then_some(NOTICE));
        shown != self.card.visible().is_some()
    }
}

impl HerdrWindow {
    /// Checks the terminal frame this window draws against the loaded config,
    /// returning whether the notice changed and the window must redraw.
    pub(crate) fn watch_symbol_font(&mut self) -> bool {
        let frame = self.live.surface.as_deref().map(|surface| &surface.frame);
        self.symbol_font_notice
            .observe(self.config.symbol_font_missing, frame)
    }

    fn copy_symbol_font_command(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(COMMAND.into()));
        self.show_flash(Flash::success("Copied the install command"), cx);
    }

    /// The command, in the terminal face on its own chip so it reads as
    /// something to run. Text in the card cannot be selected, so a click
    /// anywhere on the chip copies it, as its icon says.
    fn symbol_font_command(&self, cx: &mut Context<Self>) -> AnyElement {
        let chip = div()
            .id("symbol-font-command")
            .debug_selector(|| "symbol-font-command".into())
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.))
            .p(px(8.))
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(self.theme.active))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.copy_symbol_font_command(cx);
            }))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_font(&self.config.terminal)
                    .child(COMMAND),
            )
            .child(
                div()
                    .debug_selector(|| "symbol-font-copy".into())
                    .flex_none()
                    .child(
                        svg()
                            .path("icons/copy.svg")
                            .size(px(12.))
                            .text_color(rgb(self.theme.foreground)),
                    ),
            );
        // A row, so the chip keeps to its text and shrinks with the card.
        div()
            .min_w_0()
            .pl(px(TEXT_INDENT))
            .flex()
            .child(chip)
            .into_any_element()
    }

    pub(super) fn symbol_font_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let lines = self.symbol_font_notice.card.visible()?;
        let drawn = lines.clone();
        let command = self.symbol_font_command(cx);
        let card = self.render_diagnostic_card(
            "symbol-font-notice",
            "Herdr GPUI".into(),
            lines,
            Some(command),
            move |this, cx| {
                if this.symbol_font_notice.card.dismiss(&drawn) {
                    cx.notify();
                }
            },
            cx,
        );
        Some(card.into_any_element())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
