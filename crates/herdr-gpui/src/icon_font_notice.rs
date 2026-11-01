//! Explains missing-glyph boxes in a prompt. Fonts are never bundled, so a
//! machine without a Nerd Font draws every Private Use Area icon as a box;
//! this names the fix once a pane actually shows one.
// i18n: migrated
use crate::{config_diagnostic::ConfigDiagnostic, i18n::fl, terminal_painter::CellSeparator};
use herdr_client::protocol::{FrameData, PaneSurfaceFrame};
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// Names the fix without running it: installing fonts is the user's call.
/// One line per message, so each reads as a whole line of the card.
fn text() -> String {
    let fix = if cfg!(target_os = "macos") {
        fl!("config-diagnostic-icon-font-homebrew")
    } else {
        fl!("config-diagnostic-icon-font-download")
    };
    format!("{}\n{fix}", fl!("config-diagnostic-icon-font"))
}

/// The command that installs one, where a single command does. The card
/// draws it apart from [`text`] and copies it on a click: card text cannot be
/// selected, and a command retyped from a notice is a command mistyped.
pub(crate) const COMMAND: Option<&str> = if cfg!(target_os = "macos") {
    // i18n: data
    Some("brew install --cask font-symbols-only-nerd-font")
} else {
    None
};

#[derive(Debug, Default)]
pub(crate) struct IconFontNotice {
    /// Latched once a frame showed an icon, so a prompt that scrolls away
    /// does not take the explanation with it.
    shown_icon: bool,
    /// The last frame scanned, so a tick without a new frame costs nothing.
    /// Revisions restart with each daemon boot, so the boot is part of it.
    scanned: Option<(String, u64, u64)>,
    card: ConfigDiagnostic,
    /// Holds for the session: a reload that hides and restores the card must
    /// not bring back an explanation the user already read.
    dismissed: bool,
}

impl IconFontNotice {
    /// Follows the resolved config and the frame on screen. Frames are scanned
    /// only while the terminal has no icon font and no icon was seen yet.
    /// Returns whether the card appeared, went, or changed its language.
    pub(crate) fn observe(&mut self, missing: bool, surface: Option<&PaneSurfaceFrame>) -> bool {
        let before = self.visible().is_some();
        if missing
            && !self.shown_icon
            && let Some(surface) = surface
        {
            let scanned = self
                .scanned
                .as_ref()
                .is_some_and(|(boot, projection, revision)| {
                    *boot == surface.boot_id
                        && *projection == surface.projection_revision
                        && *revision == surface.surface_revision
                });
            if !scanned {
                self.scanned = Some((
                    surface.boot_id.clone(),
                    surface.projection_revision,
                    surface.surface_revision,
                ));
                self.shown_icon = shows_icon(&surface.frame);
            }
        }
        // The text is written once, and again only after a language switch:
        // this runs on every tick, dismissed or not.
        let rewritten = if !(missing && self.shown_icon) {
            self.card.sync_own(None);
            false
        } else if self.card.has_text() {
            self.card.relocale(|| Some(text()))
        } else {
            self.card.sync_own(Some(&text()));
            false
        };
        rewritten || before != self.visible().is_some()
    }

    pub(crate) fn visible(&self) -> Option<&Arc<[String]>> {
        self.card.visible().filter(|_| !self.dismissed)
    }

    pub(crate) fn dismiss(&mut self, lines: &Arc<[String]>) -> bool {
        let current = self.visible().is_some_and(|own| own == lines);
        self.dismissed |= current;
        current
    }

    pub(crate) fn expanded(&self) -> bool {
        self.card.expanded()
    }

    pub(crate) fn toggle(&mut self, lines: &Arc<[String]>) -> bool {
        self.visible().is_some_and(|own| own == lines) && self.card.toggle(lines)
    }
}

/// Whether any drawn cell needs an icon font. The solid separators are
/// painted as paths, so they draw without one.
fn shows_icon(frame: &FrameData) -> bool {
    frame
        .cells
        .iter()
        .filter(|cell| !cell.skip && CellSeparator::from_symbol(&cell.symbol).is_none())
        .any(|cell| cell.symbol.chars().any(is_private_use))
}

fn is_private_use(c: char) -> bool {
    // U+F8FF is Apple's logo, which the system faces of its own platform
    // draw with no icon font; elsewhere it is a box like any other.
    if c == '\u{f8ff}' && cfg!(target_os = "macos") {
        return false;
    }
    matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
}
