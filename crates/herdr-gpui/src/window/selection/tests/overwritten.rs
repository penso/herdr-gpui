use super::*;

/// A highlight kept after release marks the text it chose. Output elsewhere
/// or a recolor leaves it; text written over its rows, as a clear does,
/// retires it rather than leaving it over cells nobody chose.
#[gpui::test]
fn a_kept_highlight_retires_when_its_text_is_overwritten(cx: &mut TestAppContext) {
    let frame = |rows: &[&str], revision: u64, view: &HerdrWindow| {
        let mut frame = surface(rows, 12);
        let snapshot = view.live.snapshot.as_ref().unwrap();
        frame.boot_id = snapshot.boot_id.clone();
        frame.projection_revision = snapshot.revision;
        frame.surface_revision = revision;
        Arc::new(frame)
    };
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.surface = Some(frame(&["hello there", "second row"], 1, &view));
        view
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let (origin, cell) = view.read_with(cx, |view, _| {
        (
            view.bounds.origin,
            (view.cell_width, view.config.terminal.line_height()),
        )
    });
    let at = |column: f32, row: f32| -> Point<Pixels> {
        origin + point(px(column * cell.0), px(row * cell.1))
    };
    cx.simulate_mouse_down(at(0., 0.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(at(5., 0.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at(5., 0.), MouseButton::Left, Modifiers::default());

    view.update(cx, |view, cx| {
        assert!(view.selection_retained());
        // Another row changed, and the selected text is recolored.
        let mut next = frame(&["hello there", "third row"], 2, view);
        Arc::make_mut(&mut next).frame.cells[0].fg = 3;
        view.live.surface = Some(next);
        view.follow_selection(cx);
        assert!(view.selection_retained(), "output elsewhere keeps it");

        // The selected row now holds other text.
        view.live.surface = Some(frame(&["", "third row"], 3, view));
        view.follow_selection(cx);
        assert!(view.selection.is_none(), "a clear retires it");
    });
}

/// A selection still being dragged follows live output; only a kept one is
/// held to the text it chose.
#[gpui::test]
fn a_dragging_selection_survives_rewritten_text(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut frame = surface(&["hello there", "second row"], 12);
        let snapshot = view.live.snapshot.as_ref().unwrap();
        frame.boot_id = snapshot.boot_id.clone();
        frame.projection_revision = snapshot.revision;
        view.live.surface = Some(Arc::new(frame));
        view
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let (origin, cell) = view.read_with(cx, |view, _| {
        (
            view.bounds.origin,
            (view.cell_width, view.config.terminal.line_height()),
        )
    });
    let at = |column: f32, row: f32| -> Point<Pixels> {
        origin + point(px(column * cell.0), px(row * cell.1))
    };
    cx.simulate_mouse_down(at(0., 0.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(at(5., 0.), MouseButton::Left, Modifiers::default());
    view.update(cx, |view, cx| {
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.frame.cells[0].symbol = "H".into();
        surface.surface_revision += 1;
        view.follow_selection(cx);
        assert!(view.selection.as_ref().is_some_and(|s| s.dragging()));
    });
}

/// Widening the pane pads rows selected through to its edge with more
/// blanks. The text is unchanged, so the highlight stays.
#[gpui::test]
fn a_kept_highlight_survives_a_wider_pane(cx: &mut TestAppContext) {
    let frame = |width: u16, revision: u64, view: &HerdrWindow| {
        let mut frame = surface(&["one", "two", "three"], width);
        let snapshot = view.live.snapshot.as_ref().unwrap();
        frame.boot_id = snapshot.boot_id.clone();
        frame.projection_revision = snapshot.revision;
        frame.surface_revision = revision;
        Arc::new(frame)
    };
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.surface = Some(frame(12, 1, &view));
        view
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let (origin, cell) = view.read_with(cx, |view, _| {
        (
            view.bounds.origin,
            (view.cell_width, view.config.terminal.line_height()),
        )
    });
    let at = |column: f32, row: f32| -> Point<Pixels> {
        origin + point(px(column * cell.0), px(row * cell.1))
    };
    // From the first row's start to the third row's end: the first and
    // middle rows run to the pane's edge.
    cx.simulate_mouse_down(at(0., 0.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(at(5., 2.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at(5., 2.), MouseButton::Left, Modifiers::default());

    view.update(cx, |view, cx| {
        assert!(view.selection_retained());
        view.live.surface = Some(frame(20, 2, view));
        view.follow_selection(cx);
        assert!(view.selection_retained(), "a wider pane keeps it");
    });
}

/// Blanks inside a row's selection are text, not padding: only a row
/// selected through to the edge ignores its trailing blanks, as a copy does.
#[gpui::test]
fn a_kept_highlight_retires_when_selected_blanks_change(cx: &mut TestAppContext) {
    let frame = |row: &str, revision: u64, view: &HerdrWindow| {
        let mut frame = surface(&[row], 12);
        let snapshot = view.live.snapshot.as_ref().unwrap();
        frame.boot_id = snapshot.boot_id.clone();
        frame.projection_revision = snapshot.revision;
        frame.surface_revision = revision;
        Arc::new(frame)
    };
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.surface = Some(frame("ab  cd", 1, &view));
        view
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let (origin, cell) = view.read_with(cx, |view, _| {
        (
            view.bounds.origin,
            (view.cell_width, view.config.terminal.line_height()),
        )
    });
    let at = |column: f32, row: f32| -> Point<Pixels> {
        origin + point(px(column * cell.0), px(row * cell.1))
    };
    // "ab  ", ending mid-row on the blanks.
    cx.simulate_mouse_down(at(0., 0.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(at(4., 0.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at(4., 0.), MouseButton::Left, Modifiers::default());

    view.update(cx, |view, cx| {
        assert!(view.selection_retained());
        view.live.surface = Some(frame("ab\u{a0}\u{a0}cd", 2, view));
        view.follow_selection(cx);
        assert!(view.selection.is_none(), "changed blanks retire it");
    });
}
