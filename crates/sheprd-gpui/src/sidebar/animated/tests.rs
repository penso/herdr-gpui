#![allow(clippy::unwrap_used)]
use super::*;
use core::prelude::v1::test;
use gpui::{Render, TestAppContext};

/// The wave's still frame lights the whole grid, so a held animation under
/// reduced motion still reads as working; the loop then empties every cell,
/// as Superset's does, before it starts again.
#[test]
fn the_wave_starts_full_and_empties_within_its_loop() {
    for diagonal in 0..5 {
        assert_eq!(
            grid::wave_opacity(grid::WAVE_FULL_FRAME, diagonal as f32),
            1.,
            "{diagonal}"
        );
    }
    let frames = 0..grid::WAVE_FRAMES as usize;
    for diagonal in 0..5 {
        let lit: Vec<f32> = frames
            .clone()
            .map(|frame| grid::wave_opacity(frame as f32, diagonal as f32))
            .collect();
        assert!(lit.iter().all(|value| (0. ..=1.).contains(value)));
        assert!(lit.contains(&0.), "{diagonal} never empties");
        assert!(lit.contains(&1.), "{diagonal} never fills");
    }
}

/// A blocked grid keeps its frame lit but for one cell, which visits every
/// frame cell once a loop; the center stays dark, so it never reads as done.
#[test]
fn the_blocked_gap_visits_every_frame_cell() {
    let mut visited = Vec::new();
    for gap in grid::FRAME {
        for index in 0..9 {
            let lit = grid::chase_opacity(gap, index);
            match index {
                4 => assert_eq!(lit, 0.),
                _ if index == gap => assert!(lit > 0. && lit < 1.),
                _ => assert_eq!(lit, 1.),
            }
        }
        assert!(!visited.contains(&gap));
        visited.push(gap);
    }
    assert_eq!(visited.len(), 8);
}

/// Orca's ring holds still between twelve evenly spaced steps a turn, and
/// starts upright so a held ring shows its gap at the top.
#[test]
fn the_orca_ring_steps_twelve_times_a_turn() {
    assert_eq!(orca::step(0.), 0.);
    let steps: Vec<f32> = (0..1200).map(|ms| orca::step(ms as f32 / 1200.)).collect();
    let mut distinct = steps.clone();
    distinct.dedup();
    assert_eq!(distinct.len(), orca::STEPS as usize);
    assert!(steps.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(steps.iter().all(|turn| (0. ..1.).contains(turn)));
}

/// The blocked cell starts full, so a held one under reduced motion reads as
/// a solid mark, and its loop returns to where it began.
#[test]
fn the_blocked_braille_cell_breathes_from_full() {
    assert_eq!(BREATHE[0], "⣿");
    assert_eq!(BREATHE[1], BREATHE[BREATHE.len() - 1]);
    let dots = |glyph: &str| (glyph.chars().next().unwrap() as u32 - 0x2800).count_ones();
    let counts: Vec<u32> = BREATHE.iter().map(|glyph| dots(glyph)).collect();
    let low = counts.iter().position(|&count| count == 1).unwrap();
    assert!(counts[..=low].windows(2).all(|pair| pair[0] > pair[1]));
    assert!(counts[low..].windows(2).all(|pair| pair[0] < pair[1]));
}

/// Across one loop a stepped animation shows every frame in order, each for
/// an equal slice, so the glyph or gap drawn changes as time passes.
#[test]
fn stepped_animations_walk_every_frame_in_order() {
    for len in [BRAILLE.len(), BREATHE.len(), grid::FRAME.len()] {
        let shown: Vec<usize> = (0..len * 10)
            .map(|step| frame_index(len, step as f32 / (len * 10) as f32))
            .collect();
        let mut order = shown.clone();
        order.dedup();
        assert_eq!(order, (0..len).collect::<Vec<_>>(), "{len} frames");
        assert_eq!(frame_index(len, 1.), len - 1);
    }
    // The drawn glyph differs between neighbouring slices of the loop.
    let at = |delta: f32| BRAILLE[frame_index(BRAILLE.len(), delta)];
    assert_ne!(at(0.), at(0.15));
    assert_ne!(
        BREATHE[frame_index(BREATHE.len(), 0.)],
        BREATHE[frame_index(BREATHE.len(), 0.5)]
    );
}

#[test]
fn spinner_frames_are_single_braille_cells() {
    for frame in BRAILLE.iter().chain(&BREATHE).chain(&["⠿", "⠒", "⠂"]) {
        let mut chars = frame.chars();
        let cell = chars.next().unwrap();
        assert!(chars.next().is_none(), "{frame}");
        assert!(('\u{2800}'..='\u{28ff}').contains(&cell), "{frame}");
    }
}

struct Mark {
    set: StatusIcons,
    status: AgentStatus,
}

impl Render for Mark {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(mark(self.set, self.status, 12., 0xf9e2af))
    }
}

/// The wave and braille sets draw their settled statuses in their own cells,
/// and Orca its question bubble and ringed check, each inside its slot.
#[gpui::test]
fn settled_marks_keep_their_family_inside_the_slot(cx: &mut TestAppContext) {
    for (set, status, selector) in [
        (StatusIcons::Wave, AgentStatus::Done, "status-grid-done"),
        (StatusIcons::Wave, AgentStatus::Idle, "status-grid-idle"),
        (
            StatusIcons::Wave,
            AgentStatus::Unknown,
            "status-grid-unknown",
        ),
        (
            StatusIcons::Braille,
            AgentStatus::Done,
            "status-braille-done",
        ),
        (
            StatusIcons::Braille,
            AgentStatus::Idle,
            "status-braille-idle",
        ),
        (
            StatusIcons::Braille,
            AgentStatus::Unknown,
            "status-braille-unknown",
        ),
        (
            StatusIcons::Orca,
            AgentStatus::Blocked,
            "status-orca-question",
        ),
        (StatusIcons::Orca, AgentStatus::Done, "status-orca-done"),
    ] {
        let (_, cx) = cx.add_window_view(|_, _| Mark { set, status });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{set:?} {status:?} draws no {selector}"));
        assert!(
            bounds.size.width <= px(12.) && bounds.size.height <= px(12.),
            "{set:?} {status:?} {bounds:?}"
        );
    }
}

/// Braille cells are drawn dot for dot from the glyph's bits, and the
/// six-dot ones in three rows, so they center on the row instead of sitting
/// high in a cell of four.
#[test]
fn braille_cells_decode_their_dots_and_rows() {
    assert!(raised("⠁", 0, 0) && !raised("⠁", 0, 1));
    assert!(raised("⡀", 3, 0) && !raised("⡀", 0, 0));
    assert!(raised("⢀", 3, 1));
    for (row, column) in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)] {
        assert!(raised("⠿", row, column), "{row} {column}");
    }
    assert!(!raised("x", 0, 0));
    assert_eq!(rows(&BRAILLE), 3);
    assert_eq!(rows(&BREATHE), 4);
    // The settled marks sit on the middle of their three rows.
    for glyph in ["⠒", "⠂"] {
        assert_eq!(rows(&[glyph]), 3);
        assert!((0..2).all(|column| !raised(glyph, 0, column) && !raised(glyph, 2, column)));
    }
}

/// Three rows of dots take no more than seven tenths of the slot, so a
/// compact row's cell is no taller than its letters, and never vanish at the
/// smallest fonts.
#[test]
fn braille_dots_stay_within_seven_tenths_of_their_slot() {
    for size in [6., 10., 12., 14., 16., 20., 28.] {
        let dot = dot_diameter(size);
        assert!(dot >= 1., "{size}: {dot}");
        assert!(dot * 3. + 2. <= (size * 0.7).max(5.), "{size}: {dot}");
        assert_eq!((dot * 2.).fract(), 0., "{size}: {dot}");
    }
}

struct SizedMark {
    status: AgentStatus,
    size: f32,
}

impl Render for SizedMark {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(mark(StatusIcons::Braille, self.status, self.size, 0xf9e2af))
    }
}

/// The drawn dots sit centered inside their slot at every size, as a grid
/// of their own rows: three for the spinner and settled marks, four for the
/// breathing blocked cell.
#[gpui::test]
fn braille_dots_render_centered_inside_their_slot(cx: &mut TestAppContext) {
    for side in [8., 12., 20., 28.] {
        for (status, selector, rows) in [
            (AgentStatus::Working, "status-braille", 3.),
            (AgentStatus::Blocked, "status-braille-fill", 4.),
            (AgentStatus::Done, "status-braille-done", 3.),
            (AgentStatus::Idle, "status-braille-idle", 3.),
        ] {
            let (_, cx) = cx.add_window_view(|_, _| SizedMark { status, size: side });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let slot = cx.debug_bounds(selector).unwrap();
            let grid = cx.debug_bounds("status-braille-dots").unwrap();
            let dot = dot_diameter(side);
            let case = format!("{status:?} at {side}: slot={slot:?}, grid={grid:?}");
            assert_eq!(slot.size, size(px(side), px(side)), "{case}");
            assert_eq!(grid.size.width, px(dot * 2. + 1.), "{case}");
            assert_eq!(grid.size.height, px(dot * rows + rows - 1.), "{case}");
            assert!(slot.contains(&grid.origin), "{case}");
            assert!(slot.contains(&grid.bottom_right()), "{case}");
            assert!(
                (slot.center().x - grid.center().x).abs() <= px(0.5),
                "{case}"
            );
            assert!(
                (slot.center().y - grid.center().y).abs() <= px(0.5),
                "{case}"
            );
        }
    }
}
