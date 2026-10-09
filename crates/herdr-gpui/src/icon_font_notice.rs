//! Explains missing-glyph boxes in a prompt. Fonts are never bundled, so a
//! machine without a Nerd Font draws every Private Use Area icon as a box;
//! this names the fix once a pane actually shows one.
use crate::{config_diagnostic::ConfigDiagnostic, terminal_painter::CellSeparator};
use herdr_client::protocol::{FrameData, PaneSurfaceFrame};
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// Names the fix without running it: installing fonts is the user's call.
const TEXT: &str = if cfg!(target_os = "macos") {
    "Prompt icons need a Nerd Font, and none is installed.\n\
     brew install --cask font-symbols-only-nerd-font\n\
     Then restart Herdr GPUI."
} else {
    "Prompt icons need a Nerd Font, and none is installed.\n\
     Install Symbols Nerd Font Mono from nerdfonts.com,\n\
     then restart Herdr GPUI."
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
    /// Returns whether the card's visibility changed.
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
        self.card.sync((missing && self.shown_icon).then_some(TEXT));
        before != self.visible().is_some()
    }

    pub(crate) fn visible(&self) -> Option<&Arc<[String]>> {
        self.card.visible().filter(|_| !self.dismissed)
    }

    pub(crate) fn dismiss(&mut self, lines: &Arc<[String]>) -> bool {
        let current = self.visible().is_some_and(|own| own == lines);
        self.dismissed |= current;
        current
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
    matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
}
