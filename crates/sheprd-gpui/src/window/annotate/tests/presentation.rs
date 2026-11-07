use super::*;

#[gpui::test]
fn composer_stays_near_the_selection_and_its_controls_fit_a_narrow_pane(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(Some("working"), false));
    for columns in [80, 32] {
        view.update(cx, |view, cx| {
            let frame = Arc::make_mut(view.live.surface.as_mut().unwrap());
            frame.panes[0].rect.width = columns;
            frame.panes[0].inner_rect.width = columns;
            cx.notify();
        });
        select(&view, cx);
        write(&view, cx, "Explain this failure before retrying.");
        let card = cx.debug_bounds("terminal-note").unwrap();
        let (left, top, width, height) = view.read_with(cx, |view, _| {
            (
                view.bounds.left(),
                view.bounds.top(),
                px(f32::from(columns) * view.cell_width),
                px(24. * view.config.terminal.line_height()),
            )
        });
        let selection_bottom = view.read_with(cx, |view, _| {
            top + px(2. * view.config.terminal.line_height())
        });
        assert!((card.top() - selection_bottom - px(6.)).abs() < px(1.));
        assert!(
            card.size.height < px(180.),
            "compact like the chosen nearby mockup"
        );
        assert!(card.bottom() <= top + height);
        assert!(card.left() >= left);
        assert!(card.right() <= left + width);
        for id in [
            "terminal-note-title",
            "terminal-note-input",
            "terminal-note-hint",
            "terminal-note-send",
            "terminal-note-queue",
            "terminal-note-cancel",
        ] {
            let bounds = cx.debug_bounds(id).unwrap();
            assert!(bounds.left() >= card.left(), "{id}");
            assert!(bounds.right() <= card.right(), "{id}");
            assert!(bounds.top() >= card.top(), "{id}");
            assert!(bounds.bottom() <= card.bottom(), "{id}");
        }
        cx.simulate_keystrokes("escape");
    }
}

#[gpui::test]
fn card_buttons_cancel_queue_and_send_without_terminal_mouse_input(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    select(&view, cx);
    write(&view, cx, "Discard this.");
    let cancel = cx.debug_bounds("terminal-note-cancel").unwrap();
    cx.simulate_click(cancel.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert!(!view.terminal_notes.composing());
        assert_eq!(view.terminal_notes.queued(), 0);
    });

    select(&view, cx);
    write(&view, cx, "First note.");
    let queue = cx.debug_bounds("terminal-note-queue").unwrap();
    cx.simulate_click(queue.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert!(!view.terminal_notes.composing());
        assert_eq!(view.terminal_notes.queued(), 1);
    });

    select(&view, cx);
    write(&view, cx, "Second note.");
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0].mouse_reporting = true;
    });
    let send = cx.debug_bounds("terminal-note-send").unwrap();
    cx.simulate_click(send.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert!(!view.terminal_notes.composing());
        assert_eq!(view.terminal_notes.queued(), 0);
    });
    let copied = cx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap();
    assert!(copied.contains("First note."));
    assert!(copied.contains("Second note."));
    assert!(!copied.contains("Discard this."));
}

#[gpui::test]
fn the_pill_and_composer_flip_above_a_selection_at_the_bottom_edge(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    view.update(cx, |view, _| {
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        let start = 23 * usize::from(surface.frame.width) + 74;
        for (index, c) in "failed".chars().enumerate() {
            surface.frame.cells[start + index].symbol = c.to_string();
        }
    });
    let (origin, cell) = view.read_with(cx, |view, _| {
        (
            view.bounds.origin,
            (view.cell_width, view.config.terminal.line_height()),
        )
    });
    let at = |column| origin + point(px(column * cell.0), px(23. * cell.1 + 1.));
    cx.simulate_mouse_down(at(74.), MouseButton::Left, shift_selection::shift());
    cx.simulate_mouse_move(at(80.), MouseButton::Left, shift_selection::shift());
    cx.simulate_mouse_up(at(80.), MouseButton::Left, shift_selection::shift());
    let pill = cx.debug_bounds("terminal-annotate").unwrap();
    let row_top = origin.y + px(23. * cell.1);
    assert!(pill.bottom() <= row_top);
    assert!(pill.right() <= origin.x + px(80. * cell.0));
    cx.simulate_click(pill.center(), Modifiers::default());
    let composer = cx.debug_bounds("terminal-note").unwrap();
    assert!(composer.bottom() <= row_top);
    assert!(composer.top() >= origin.y);
    assert!(composer.right() <= origin.x + px(80. * cell.0));
    assert!(composer.left() >= origin.x);
    let input = cx.debug_bounds("terminal-note-input").unwrap();
    let send = cx.debug_bounds("terminal-note-send").unwrap();
    assert!(composer.contains(&input.center()));
    assert!(composer.contains(&send.center()));
}
