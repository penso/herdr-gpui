//! Linked Git worktrees told apart from their repository's main checkout.
//!
//! The daemon already reports, per workspace, whether its checkout is a linked
//! worktree (its `.git` is a file pointing into another repository's
//! `worktrees/`), so nothing here reads Git. In the sidebar a linked worktree
//! hangs from its main checkout on the tree's guide lines, in every layout,
//! and one standing alone (its main checkout not open) takes the branch glyph
//! instead, since it has no line to hang from. In the workspace, when
//! `apply_to` asks for `"terminal"`, a hairline runs where its tab strip
//! meets its terminals, and none while the strip is hidden, since it would
//! then cover cells; it is off by default. The main checkout keeps the
//! theme as drawn, so the odd one out is the copy, not the original.
//!
//! The guides are structure, not identity, so they keep the tree's own muted
//! stroke and stay quiet however many worktrees are open. The glyph and the
//! rule carry the hue: the theme's blue, as the notes kept on worktrees draw
//! it, never [`Theme::primary`], which already means "selected".

use crate::config::Theme;
use gpui::{Div, ParentElement, Styled, div, px, rgb};
use herdr_client::protocol::{ClientShellSnapshot, ClientShellWorkspace};
use serde::Deserialize;

#[cfg(test)]
mod tests;

/// Thickness of the rule along a linked worktree's tab strip, in logical
/// pixels: as thin as reads at a glance without becoming a border.
pub(crate) const RULE: f32 = 2.;

/// The palette entry the default hue comes from: the theme's blue.
const BLUE: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("worktrees.tint must be a #rgb or #rrggbb colour, not {0:?}")]
    InvalidTint(String),
}

/// Where a linked worktree's marks are drawn.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    /// The sidebar: guide lines in the layouts that draw no tree of their
    /// own, and the branch glyph on a worktree standing alone.
    Sidebar,
    /// The edge between the workspace's tab strip and its terminals.
    Terminal,
}

/// `[worktrees]`, resolved: an optional hue and the surfaces that show it.
/// By default only the sidebar does; the rule along the tab strip is opt-in.
/// `apply_to = []` turns every mark off, leaving the sidebar as Herdr draws
/// it without them: the flat layouts keep their tree, the rest none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Worktrees {
    /// A configured hue for the glyph and the rule, as `0xRRGGBB`. Unset,
    /// the theme's blue.
    pub tint: Option<u32>,
    pub sidebar: bool,
    pub terminal: bool,
}

impl Default for Worktrees {
    fn default() -> Self {
        Self {
            tint: None,
            sidebar: true,
            terminal: false,
        }
    }
}

impl Worktrees {
    /// The marks for `surfaces` in `tint`, or the theme's hue without one.
    pub(crate) fn new(tint: Option<u32>, surfaces: &[Surface]) -> Self {
        Self {
            tint,
            sidebar: surfaces.contains(&Surface::Sidebar),
            terminal: surfaces.contains(&Surface::Terminal),
        }
    }

    /// The hue as a mark on `theme`'s chrome: the glyph and the rule. Its
    /// lightness moves only as far as the contrast setting needs to read on
    /// the background, the surface, a selected row, and Herdr's own sidebar
    /// colour when the theme sets one, so a configured colour keeps its hue.
    pub(crate) fn mark(&self, theme: &Theme) -> u32 {
        let mark = theme.ink(self.tint.unwrap_or(theme.palette[BLUE]));
        match theme.sidebar {
            Some(sidebar) => crate::contrast::ink_on_chrome(
                mark,
                [sidebar, theme.surface, theme.active],
                theme.contrast.mark_ratio(),
            ),
            None => mark,
        }
    }

    /// The branch glyph's colour on `theme` for a linked worktree standing
    /// alone, unless the sidebar does not show the marks.
    pub(crate) fn glyph(&self, theme: &Theme) -> Option<u32> {
        self.sidebar.then(|| self.mark(theme))
    }

    /// The rule along the edge of a linked worktree's terminals, when they
    /// show one. Absolutely placed: the caller pins it to the edge, so the
    /// terminal keeps its size whichever workspace is shown.
    pub(crate) fn rule(&self, linked: bool, theme: &Theme) -> Option<Div> {
        (linked && self.terminal).then(|| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .h(px(RULE))
                .bg(rgb(self.mark(theme)))
        })
    }
}

/// Whether `workspace` is a linked worktree rather than a main checkout or a
/// folder outside Git, as the daemon reports it.
pub(crate) fn is_linked(workspace: &ClientShellWorkspace) -> bool {
    workspace
        .worktree
        .as_ref()
        .is_some_and(|tree| tree.is_linked_worktree)
}

/// Whether the workspace `snapshot` shows is a linked worktree.
pub(crate) fn focused_is_linked(snapshot: &ClientShellSnapshot) -> bool {
    snapshot
        .workspaces
        .iter()
        .find(|workspace| Some(&workspace.workspace_id) == snapshot.focused_workspace_id.as_ref())
        .is_some_and(is_linked)
}

/// A group's strip with its rule, along the strip's edge that meets the
/// terminals and inside the chrome so no cell is covered. `below` is a strip
/// under the terminals. A group that shows no strip has no chrome edge, and
/// a rule along its top would lie over the first row of cells, so it shows
/// none.
pub(crate) fn place_rule(strip: Option<Div>, rule: Option<Div>, below: bool) -> Option<Div> {
    let strip = strip?;
    Some(match rule {
        Some(rule) => strip.child(if below { rule.top_0() } else { rule.bottom_0() }),
        None => strip,
    })
}
