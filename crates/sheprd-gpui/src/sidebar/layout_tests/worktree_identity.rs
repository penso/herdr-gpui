//! Linked worktrees hang from their main checkout on guide lines in every
//! layout, and one standing alone leads with the branch glyph; the main
//! checkout and plain folders keep the theme as drawn.
use super::*;
use crate::{
    config::{Density, LayoutMode, StatusIcons, Style},
    sidebar::{STATUS_WIDTH, layout::SidebarMetrics},
    worktree_identity::{Surface, Worktrees},
};

/// The fixture's repository: `agent-launcher` is the main checkout (w3) with
/// two linked worktrees nested under it, w4 and then w5, the last child. w2
/// becomes a linked worktree of a repository whose main checkout is not
/// open, so it stands alone. `herdr` is a folder outside Git.
const MAIN: &str = "agent-launcher";
const FIRST: &str = "sidebar-child";
const LAST: &str = "sidebar-child-with-a-long-readable-branch-name";
const ALONE: &str = "launcher-hotfix";
const PLAIN: &str = "herdr";

/// Bounds of the element probed as `selector`. The probe API takes static
/// names, and these are built per row, so each one is leaked: a test's
/// handful of short strings.
fn probe(cx: &mut gpui::VisualTestContext, selector: String) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
}

fn draw(
    cx: &mut gpui::TestAppContext,
    mode: LayoutMode,
    worktrees: Worktrees,
    repository_icons: bool,
) -> &mut gpui::VisualTestContext {
    draw_rows(cx, mode, worktrees, repository_icons, None)
}

/// [`draw`], with the workspace rows Herdr's sidebar config names, if any.
fn draw_rows<'a>(
    cx: &'a mut gpui::TestAppContext,
    mode: LayoutMode,
    worktrees: Worktrees,
    repository_icons: bool,
    rows: Option<&str>,
) -> &'a mut gpui::VisualTestContext {
    draw_with(cx, mode, worktrees, repository_icons, rows, |_| {})
}

/// [`draw_rows`], with `configure` applied to the window's config last.
fn draw_with<'a>(
    cx: &'a mut gpui::TestAppContext,
    mode: LayoutMode,
    worktrees: Worktrees,
    repository_icons: bool,
    rows: Option<&str>,
    configure: impl FnOnce(&mut crate::config::Config),
) -> &'a mut gpui::VisualTestContext {
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.layout.mode = mode;
            if let Some(rows) = rows {
                view.config.sidebar_layout =
                    toml::from_str(&format!("[spaces]\nrows = {rows}")).unwrap();
            }
            view.config.worktrees = worktrees;
            view.config.show_repository_icons = repository_icons;
            configure(&mut view.config);
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            let alone = &mut snapshot.workspaces[2];
            alone.label = ALONE.into();
            alone.worktree = Some(ClientShellWorktree {
                key: "/fixture/elsewhere/.git".into(),
                label: "elsewhere".into(),
                is_linked_worktree: true,
            });
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx
}

fn layouts() -> impl Iterator<Item = LayoutMode> {
    LayoutMode::ALL
        .into_iter()
        .filter(|mode| mode.lists_workspaces())
}

/// Every layout hangs the nested worktrees from guide lines, and only them:
/// the main checkout, a worktree standing alone and a plain folder have no
/// line to hang from.
#[gpui::test]
fn every_layout_hangs_nested_worktrees_from_guides(cx: &mut gpui::TestAppContext) {
    for mode in layouts() {
        let cx = draw(cx, mode, Worktrees::default(), true);
        for key in [FIRST, LAST] {
            assert!(probe(cx, format!("tree-{key}")).is_some(), "{mode}: {key}");
        }
        for key in [MAIN, ALONE, PLAIN] {
            assert!(probe(cx, format!("tree-{key}")).is_none(), "{mode}: {key}");
        }
    }
}

/// Where the guide lies in each layout: the trunk drops from the leading
/// edge of the parent's first mark, the tick ends where the child's own
/// leading mark begins, and the trunk runs unbroken from the main checkout
/// down through every child, across the gaps between rounded rows and cards.
#[gpui::test]
fn guides_run_from_the_main_checkout_to_each_childs_leading_mark(cx: &mut gpui::TestAppContext) {
    for mode in layouts() {
        let cx = draw(cx, mode, Worktrees::default(), true);
        let first = probe(cx, format!("tree-{FIRST}")).unwrap();
        let last = probe(cx, format!("tree-{LAST}")).unwrap();
        let parent = probe(cx, format!("row-{MAIN}")).unwrap();
        let (drop_from, reach) = match mode {
            // The parent's glyph, centred in its slot at 70% of it.
            LayoutMode::Superset => {
                let (parent, child) = (
                    probe(cx, format!("icon-{MAIN}")).unwrap(),
                    probe(cx, format!("icon-{FIRST}")).unwrap(),
                );
                let inset = parent.size.width * 0.15;
                (parent.left() + inset, child.left() + inset)
            }
            // The parent's status dot, centred in its 12px column inside its
            // card's border and padding.
            LayoutMode::Orca => (
                parent.left() + px(1. + 6. + 2.),
                probe(cx, format!("row-{FIRST}")).unwrap().left() + px(1. + 6. + 2.),
            ),
            // The parent's label column, as Herdr's rows lay it out after
            // the status dot and the gap; the tick ends at the child's dot.
            LayoutMode::Minimal => {
                let gap = SidebarMetrics::for_mode(mode).gap;
                (
                    probe(cx, format!("name-{MAIN}")).unwrap().left(),
                    probe(cx, format!("name-{FIRST}")).unwrap().left() - px(STATUS_WIDTH + gap),
                )
            }
            _ => (
                probe(cx, format!("column-{MAIN}")).unwrap().left(),
                probe(cx, format!("mark-{FIRST}")).unwrap().left(),
            ),
        };
        // Layout rounds to the pixel grid, and the lines then snap to
        // device pixels, so half a pixel is as close as a box can say.
        for guide in [first, last] {
            assert!((guide.left() - drop_from).abs() <= px(0.5), "{mode}");
            assert!((guide.right() - reach).abs() <= px(0.5), "{mode}");
        }
        // Unbroken: the first child's guide starts where the main checkout's
        // row ends, and the last child's where the first child's ends.
        assert_eq!(first.top(), parent.bottom(), "{mode}");
        assert_eq!(last.top(), first.bottom(), "{mode}");
        // The guide spans its row, and never reaches into the label beyond.
        for key in [FIRST, LAST] {
            let row = probe(cx, format!("row-{key}")).unwrap();
            let guide = probe(cx, format!("tree-{key}")).unwrap();
            assert_eq!((guide.top(), guide.bottom()), (row.top(), row.bottom()));
            assert!(guide.right() < probe(cx, format!("name-{key}")).unwrap().left());
        }
    }
}

/// A larger `[sidebar] size` or a wider indicator set widens the status
/// slot, which moves the parent's name along, and children step in as far,
/// so no layout loses a guide to it: each keeps the width the dots give it
/// at the default size, short of the child's name. With dots, Minimal's
/// still runs from the parent's name to the child's dot.
#[gpui::test]
fn guides_keep_their_room_at_large_sidebar_sizes(app: &mut gpui::TestAppContext) {
    let keys = [FIRST, LAST];
    let default = crate::config::Config::default().sidebar.size;
    let cases = [(StatusIcons::Dots, 24.), (StatusIcons::Dots, 48.)]
        .into_iter()
        .chain(StatusIcons::ALL.iter().map(|&icons| (icons, default)));
    let cases: Vec<_> = cases.collect();
    for mode in layouts() {
        let widths = {
            let cx = draw(app, mode, Worktrees::default(), true);
            keys.map(|key| probe(cx, format!("tree-{key}")).unwrap().size.width)
        };
        for &(icons, size) in &cases {
            let case = format!("{mode} with {icons:?} at {size}");
            let cx = draw_with(app, mode, Worktrees::default(), true, None, |config| {
                config.status_icons = icons;
                config.sidebar.size = size;
            });
            for (key, width) in keys.into_iter().zip(widths) {
                let guide = probe(cx, format!("tree-{key}"))
                    .unwrap_or_else(|| panic!("{case}: no guide for {key}"));
                assert!(
                    (guide.size.width - width).abs() <= px(0.5),
                    "{case}: {key} is {:?}, {width:?} with dots at the default",
                    guide.size.width
                );
                let name = probe(cx, format!("name-{key}")).unwrap();
                assert!(guide.right() < name.left(), "{case}: {key}");
            }
            if mode == LayoutMode::Minimal && icons == StatusIcons::Dots {
                let font = crate::config::FontConfig {
                    size,
                    ..crate::config::Config::default().sidebar
                };
                let dot = crate::sidebar::scaled(&font, STATUS_WIDTH);
                let gap = SidebarMetrics::for_mode(mode).gap;
                let parent = probe(cx, format!("name-{MAIN}")).unwrap().left();
                for key in keys {
                    let guide = probe(cx, format!("tree-{key}")).unwrap();
                    let dot_left = probe(cx, format!("name-{key}")).unwrap().left() - px(dot + gap);
                    assert!((guide.left() - parent).abs() <= px(0.5), "{case}: {key}");
                    assert!((guide.right() - dot_left).abs() <= px(0.5), "{case}: {key}");
                }
            }
        }
    }
}

/// Herdr's flat rows draw their tree on their own; every other layout draws
/// guides for `[worktrees]` and none without it, as before worktrees asked
/// for them.
#[test]
fn only_the_flat_layouts_draw_guides_without_worktrees() {
    for mode in LayoutMode::ALL {
        let preset = SidebarMetrics::for_mode(mode);
        let own = matches!(
            mode,
            LayoutMode::Classic {
                style: Style::Flat,
                ..
            } | LayoutMode::Devices
        );
        assert_eq!(preset.tree_lines, own, "{mode}");
        assert_eq!(preset.with_guides(false).tree_lines, own, "{mode}");
        assert!(preset.with_guides(true).tree_lines, "{mode}");
    }
}

/// `apply_to` without `sidebar` leaves every layout as Herdr draws it
/// without the marks: the flat ones keep their tree, the rest drop their
/// guides, and the row standing alone gets the owner's avatar back.
#[gpui::test]
fn turning_the_sidebar_off_leaves_rows_unmarked(cx: &mut gpui::TestAppContext) {
    for mode in [
        LayoutMode::default(),
        LayoutMode::new(Density::Comfortable, Style::Rounded),
        LayoutMode::Superset,
        LayoutMode::Orca,
        LayoutMode::Minimal,
    ] {
        let cx = draw(cx, mode, Worktrees::new(None, &[Surface::Terminal]), true);
        let flat = mode == LayoutMode::default();
        assert_eq!(probe(cx, format!("tree-{FIRST}")).is_some(), flat, "{mode}");
        assert!(
            probe(cx, format!("worktree-icon-{ALONE}")).is_none(),
            "{mode}"
        );
    }
    let cx = draw(cx, LayoutMode::default(), Worktrees::new(None, &[]), true);
    assert!(probe(cx, format!("github-{ALONE}")).is_some());
}

/// In Herdr's own rows a linked worktree standing alone takes the branch
/// glyph in the slot its owner's avatar used, so it never relies on colour
/// alone, even with the repository icons off. A nested child hangs from its
/// guide and keeps its geometry.
#[gpui::test]
fn a_linked_worktree_standing_alone_leads_with_the_branch_glyph(cx: &mut gpui::TestAppContext) {
    for mode in [
        LayoutMode::from(Density::Comfortable),
        LayoutMode::new(Density::Comfortable, Style::Rounded),
        LayoutMode::default(),
    ] {
        for icons in [true, false] {
            let cx = draw(cx, mode, Worktrees::default(), icons);
            let glyph = probe(cx, format!("worktree-icon-{ALONE}")).unwrap();
            let name = probe(cx, format!("name-{ALONE}")).unwrap();
            let row = probe(cx, format!("row-{ALONE}")).unwrap();
            assert!(glyph.right() <= name.left(), "{mode}");
            assert!(name.right() <= row.right(), "{mode}");
            assert_eq!(glyph.size, size(px(12.), px(12.)), "{mode}");
            assert!(probe(cx, format!("github-{ALONE}")).is_none());
            // The main checkout's avatar slot is where it was.
            let main = probe(cx, format!("github-{MAIN}"));
            assert_eq!(main.is_some(), icons, "{mode}");
            if let Some(main) = main {
                assert_eq!(main.left(), glyph.left(), "{mode}");
            }
            for key in [MAIN, FIRST, LAST, PLAIN] {
                assert!(
                    probe(cx, format!("worktree-icon-{key}")).is_none(),
                    "{mode}: {key}"
                );
            }
        }
    }
}

/// The layouts without Herdr's avatar slot lead a linked worktree standing
/// alone with the branch glyph too, on their own rows and on configured
/// ones, so it never has neither a guide nor a glyph.
#[gpui::test]
fn rows_without_an_avatar_slot_lead_with_the_branch_glyph(cx: &mut gpui::TestAppContext) {
    for mode in [LayoutMode::Minimal, LayoutMode::Orca] {
        for rows in [None, Some(r#"[["state_icon", "workspace"]]"#)] {
            let cx = draw_rows(cx, mode, Worktrees::default(), true, rows);
            let glyph = probe(cx, format!("worktree-icon-{ALONE}"));
            let glyph = glyph.unwrap_or_else(|| panic!("{mode}: {rows:?}: no glyph"));
            let name = probe(cx, format!("name-{ALONE}")).unwrap();
            let row = probe(cx, format!("row-{ALONE}")).unwrap();
            assert!(glyph.right() <= name.left(), "{mode}: {rows:?}");
            assert!(row.left() <= glyph.left(), "{mode}: {rows:?}");
            assert!(glyph.top() >= row.top() && glyph.bottom() <= row.bottom());
            for key in [MAIN, FIRST, LAST, PLAIN] {
                assert!(
                    probe(cx, format!("worktree-icon-{key}")).is_none(),
                    "{mode}: {rows:?}: {key}"
                );
            }
        }
    }
}
