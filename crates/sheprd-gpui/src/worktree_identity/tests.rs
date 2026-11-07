#![allow(clippy::unwrap_used)]

use super::*;
use crate::contrast::{Contrast, ratio};

mod placement;

/// Every theme the app ships, plus a theme with Herdr's own sidebar colour,
/// each at both contrast levels.
fn themes() -> Vec<(String, Theme)> {
    let mut sidebar = Theme::builtin("Catppuccin Mocha").unwrap();
    sidebar.sidebar = Some(0x2a2b3c);
    Theme::BUILTIN_NAMES
        .iter()
        .map(|name| ((*name).to_owned(), Theme::builtin(name).unwrap()))
        .chain([("sidebar_bg".to_owned(), sidebar)])
        .flat_map(|(name, theme)| {
            [Contrast::Standard, Contrast::High].map(|contrast| {
                (
                    format!("{name} {contrast:?}"),
                    theme.clone().with_contrast(contrast),
                )
            })
        })
        .collect()
}

/// The glyph and the rule are meaningful graphics: WCAG asks 3:1 of them,
/// and high contrast asks 4.5:1, against every chrome they sit on.
#[test]
fn the_mark_reads_on_every_theme_at_its_contrast_level() {
    let worktrees = Worktrees::default();
    for (name, theme) in themes() {
        let mark = worktrees.mark(&theme);
        let target = theme.contrast.mark_ratio();
        for chrome in [
            theme.background,
            theme.surface,
            theme.active,
            theme.sidebar_background(),
        ] {
            let ratio = ratio(mark, chrome);
            assert!(
                ratio >= target - 0.01,
                "{name}: {mark:06x} on {chrome:06x} is {ratio:.2}"
            );
        }
    }
}

/// The mark is the theme's own blue, so a theme's worktrees and its notes
/// share one hue, and it is never the selection accent. High contrast can
/// lift both to the same white on a dark theme, so the glyph's shape carries
/// the meaning there.
#[test]
fn the_default_hue_is_the_themes_blue_and_never_the_selection_accent() {
    let worktrees = Worktrees::default();
    for (name, theme) in themes() {
        let mark = worktrees.mark(&theme);
        assert_eq!(mark, theme.ink(theme.palette[4]), "{name}");
        if theme.contrast == Contrast::High {
            continue;
        }
        let accent = theme.ink(theme.primary());
        let apart = degrees_apart(mark, accent);
        assert!(
            apart >= 25.,
            "{name}: {mark:06x} is {apart:.0} degrees from the accent {accent:06x}"
        );
    }
}

/// A configured colour keeps its hue: dark or pale, it only moves as far as
/// it must to read.
#[test]
fn a_configured_tint_keeps_its_hue() {
    let theme = Theme::builtin("Catppuccin Latte").unwrap();
    for tint in [0x2a3350, 0xf5c2e7, 0x40a02b] {
        let worktrees = Worktrees {
            tint: Some(tint),
            ..Worktrees::default()
        };
        let mark = worktrees.mark(&theme);
        assert_eq!(mark, theme.ink(tint));
        assert!(ratio(mark, theme.background) >= 3. - 0.01, "{tint:06x}");
    }
}

/// The glyph and the rule are one mark, which keeps its contrast under
/// high contrast: the guides are the tree's own and need nothing here.
#[test]
fn the_glyph_and_the_rule_share_the_mark() {
    for (name, theme) in themes() {
        let worktrees = Worktrees::default();
        assert_eq!(
            worktrees.glyph(&theme),
            Some(worktrees.mark(&theme)),
            "{name}"
        );
    }
}

#[test]
fn surfaces_switch_each_mark_on_its_own() {
    let theme = Theme::default();
    for (surfaces, sidebar, terminal) in [
        (&[Surface::Sidebar, Surface::Terminal][..], true, true),
        (&[Surface::Sidebar][..], true, false),
        (&[Surface::Terminal][..], false, true),
        (&[][..], false, false),
    ] {
        let worktrees = Worktrees::new(None, surfaces);
        assert_eq!(worktrees.glyph(&theme).is_some(), sidebar, "{surfaces:?}");
        assert_eq!(
            worktrees.rule(true, &theme).is_some(),
            terminal,
            "{surfaces:?}"
        );
        // A main checkout never gets a rule, whatever is switched on.
        assert!(worktrees.rule(false, &theme).is_none());
    }
}

fn workspace(id: &str, worktree: Option<bool>) -> ClientShellWorkspace {
    serde_json::from_value(serde_json::json!({
        "workspace_id": id, "active_tab_id": "t0", "new_workspace_cwd": "/tmp",
        "number": 1, "label": id, "custom_label": false, "branch": "main",
        "worktree": worktree.map(|linked| serde_json::json!({
            "key": "/repo/.git", "label": "repo", "is_linked_worktree": linked
        })),
        "tokens": [], "focused": false, "agent_status": "idle"
    }))
    .unwrap()
}

/// Only a linked worktree is marked: not the main checkout, and not a
/// folder outside Git.
#[test]
fn only_linked_worktrees_are_marked() {
    assert!(is_linked(&workspace("linked", Some(true))));
    assert!(!is_linked(&workspace("main", Some(false))));
    assert!(!is_linked(&workspace("folder", None)));
}

#[test]
fn the_focused_workspace_decides_the_terminal_rule() {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(6);
    for (focused, linked) in [("w0", false), ("w3", false), ("w4", true), ("w5", true)] {
        snapshot.focused_workspace_id = Some(focused.into());
        assert_eq!(focused_is_linked(&snapshot), linked, "{focused}");
    }
    snapshot.focused_workspace_id = None;
    assert!(!focused_is_linked(&snapshot));
}

fn degrees_apart(first: u32, second: u32) -> f32 {
    let apart = (crate::contrast::hue_degrees(first) - crate::contrast::hue_degrees(second))
        .abs()
        .rem_euclid(360.);
    apart.min(360. - apart)
}
