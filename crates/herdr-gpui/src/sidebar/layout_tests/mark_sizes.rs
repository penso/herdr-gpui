use super::*;
use crate::config::{LayoutMode, StatusIcons};
use crate::git::{Counted, Status};

/// The marks a row draws beside its text: the status dot (Superset's on its
/// slot), the repository icon or Superset's slot, the agent's icon, and the
/// uncommitted work mark.
const MARKS: &[&str] = &[
    "mark-herdr",
    "dot-herdr",
    "github-herdr",
    "icon-herdr",
    "agent-icon-agent-p0",
    "dirty-agent-launcher",
];

/// Each mark's bounds, the fold chevron's, and what they sit beside, at
/// `font_size`.
fn measure(
    view: &Entity<HerdrWindow>,
    font_size: f32,
    cx: &mut gpui::VisualTestContext,
) -> Vec<(&'static str, Option<Bounds<Pixels>>)> {
    view.update(cx, |view, cx| {
        view.config.sidebar.size = font_size;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    MARKS
        .iter()
        .copied()
        .chain([
            "collapse-3",
            "name-herdr",
            "row-herdr",
            "row-agent-launcher",
        ])
        .map(|probe| (probe, cx.debug_bounds(probe)))
        .collect()
}

/// Raising `[sidebar] size` grows the marks with the text in every layout:
/// at twice the default size each mark is twice as large, within the
/// rounding to whole pixels, and still inside its row, while the default
/// size keeps the sizes the marks always had. Herdr's rows keep the dot and
/// the repository icon centered on the name's line, and the name clear of
/// the icon by its scaled reserve.
#[gpui::test]
fn marks_grow_with_the_sidebar_font(cx: &mut gpui::TestAppContext) {
    let default = crate::config::Config::default().sidebar.size;
    // Devices draws Herdr's rows under device headers, which this fixture has none of.
    for mode in LayoutMode::ALL
        .into_iter()
        .filter(|mode| *mode != LayoutMode::Devices)
    {
        // A fresh window per layout: GPUI keeps debug bounds from earlier
        // frames, so another layout's probes would linger.
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            view.config.layout.mode = mode;
            view.config.status_icons = StatusIcons::Dots;
            view.sidebar_width = Some(400.);
            let input = crate::pull_request::Input {
                checkout: None,
                repo_key: Some(REPO_KEY.into()),
                branch: "develop".into(),
            };
            let counted = Counted {
                lines: Status {
                    additions: 3,
                    deletions: 1,
                    untracked: 0,
                },
                dirty: true,
            };
            view.git
                .seed_counts(input, counted, std::time::Instant::now());
            view
        });
        cx.simulate_resize(size(px(800.), px(900.)));
        cx.run_until_parked();
        let small = measure(&view, default, cx);
        let large = measure(&view, 2. * default, cx);
        let bounds = |measured: &[(&str, Option<Bounds<Pixels>>)], probe: &str| {
            measured
                .iter()
                .find(|(name, _)| *name == probe)
                .and_then(|(_, bounds)| *bounds)
        };
        let mut drawn = 0;
        for &probe in MARKS {
            let (Some(small), Some(large)) = (bounds(&small, probe), bounds(&large, probe)) else {
                continue;
            };
            drawn += 1;
            for (small, large) in [
                (small.size.width, large.size.width),
                (small.size.height, large.size.height),
            ] {
                assert!(
                    (large - small * 2.).abs() <= px(1.),
                    "{mode:?}: {probe} is {large:?} at twice the size of {small:?}"
                );
            }
        }
        // The chevron's column grows with its glyph. Herdr's and Minimal's
        // rows keep the gap before it, so theirs grows by its default width.
        let fold = |measured: &[(&str, Option<Bounds<Pixels>>)]| {
            bounds(measured, "collapse-3").map(|fold| fold.size.width)
        };
        if let (Some(small), Some(large)) = (fold(&small), fold(&large)) {
            drawn += 1;
            let growth = match mode {
                LayoutMode::Classic { .. } | LayoutMode::Minimal => {
                    px(crate::sidebar::ARROW_RESERVE)
                }
                _ => small,
            };
            assert!(
                (large - small - growth).abs() <= px(1.),
                "{mode:?}: the chevron's column is {large:?}, from {small:?}"
            );
        }
        assert!(drawn > 0, "{mode:?}: no marks drawn");
        // Each mark stays within the row it sits on at the larger size.
        for (probe, row) in [
            ("mark-herdr", "row-herdr"),
            ("dot-herdr", "row-herdr"),
            ("github-herdr", "row-herdr"),
            ("icon-herdr", "row-herdr"),
            ("dirty-agent-launcher", "row-agent-launcher"),
        ] {
            let (Some(mark), Some(row)) = (bounds(&large, probe), bounds(&large, row)) else {
                continue;
            };
            assert!(
                mark.top() >= row.top() && mark.bottom() <= row.bottom(),
                "{mode:?}: {probe} {mark:?} leaves {row} {row:?}"
            );
        }
        if !matches!(mode, LayoutMode::Classic { .. }) {
            continue;
        }
        for measured in [&small, &large] {
            let name = bounds(measured, "name-herdr").unwrap();
            for probe in ["mark-herdr", "github-herdr"] {
                let mark = bounds(measured, probe).unwrap();
                assert!(
                    (mark.center().y - name.center().y).abs() <= px(0.5),
                    "{mode:?}: {probe} {mark:?} is off the name's line {name:?}"
                );
            }
        }
        let reserve = |measured: &[(&str, Option<Bounds<Pixels>>)]| {
            bounds(measured, "name-herdr").unwrap().left()
                - bounds(measured, "github-herdr").unwrap().left()
        };
        assert_eq!(reserve(&small), px(crate::sidebar::ICON_RESERVE));
        assert_eq!(reserve(&large), px(2. * crate::sidebar::ICON_RESERVE));
    }
}
