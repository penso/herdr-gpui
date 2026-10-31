//! Pull requests as the app shows them. `herdr-github` reads them on its
//! worker; the app shares one store across its windows and colors badges.
// i18n: migrated

pub(crate) use herdr_github::pull_request::*;
use std::{cell::RefCell, rc::Rc};

/// The store the app's windows share, as a GPUI global. It is only touched on
/// the UI thread.
#[derive(Default)]
pub(crate) struct Shared(pub(crate) Rc<RefCell<Store>>);

impl gpui::Global for Shared {}

/// A badge's color, shared by sidebar and titlebar badges and the workspace
/// menu, inked to read on the chrome it sits on.
pub(crate) fn color(pr: &PullRequest, theme: &crate::config::Theme) -> u32 {
    tone_color(pr.tone(), theme)
}

fn tone_color(tone: Tone, theme: &crate::config::Theme) -> u32 {
    theme.ink(match tone {
        Tone::Attention => theme.palette[1],
        Tone::Merged => theme.palette[5],
        Tone::Muted => theme.muted,
        Tone::Ready => theme.palette[2],
        // ANSI's extended orange distinguishes a blocked/behind branch from
        // pending checks without borrowing a lifecycle color.
        Tone::Held => theme.palette[208],
        Tone::Waiting => theme.palette[3],
    })
}

#[cfg(test)]
mod tests;
