use super::shift_selection::offer;
use super::*;

#[gpui::test]
fn an_offer_cannot_quote_rewritten_text_even_before_the_next_tick(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    for poll_first in [false, true] {
        offer(&view, cx);
        let pill = cx.debug_bounds("terminal-annotate").unwrap();
        view.update(cx, |view, _| {
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.surface_revision += 1;
            let index = usize::from(surface.frame.width);
            surface.frame.cells[index].symbol = if poll_first { "f" } else { "X" }.into();
        });
        if poll_first {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| view.follow_terminal_note(window, cx))
            });
            assert!(view.read_with(cx, |view, _| view.terminal_notes.offer.is_none()));
        } else {
            cx.simulate_click(pill.center(), Modifiers::default());
        }
        assert!(!view.read_with(cx, |view, _| view.terminal_notes.composing()));
    }
}

#[gpui::test]
fn an_old_offer_cannot_cross_a_boot_or_connection_change(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    offer(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.selection_epoch += 1;
            view.open_terminal_annotation_offer(window, cx);
            assert!(!view.terminal_notes.composing());
        })
    });
    offer(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).boot_id = "new-boot".into();
            Arc::make_mut(view.live.surface.as_mut().unwrap()).boot_id = "new-boot".into();
            view.open_terminal_annotation_offer(window, cx);
            assert!(!view.terminal_notes.composing());
        })
    });
}

#[gpui::test]
fn a_closed_pane_retires_its_offer(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    offer(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.panes.clear();
            surface.surface_revision += 1;
            view.follow_terminal_note(window, cx);
            assert!(view.terminal_notes.offer.is_none());
            assert!(!view.terminal_notes.composing());
        })
    });
}

#[gpui::test]
fn selecting_again_does_not_replace_a_message_being_written(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    select(&view, cx);
    write(&view, cx, "This is just an observation.");
    offer(&view, cx);
    view.read_with(cx, |view, cx| {
        assert!(view.terminal_notes.offer.is_none());
        let composer = view.terminal_notes.composer.as_ref().unwrap();
        assert_eq!(
            composer.input.read(cx).text(),
            "This is just an observation."
        );
        assert_eq!(composer.quote, "failed");
    });
}

#[gpui::test]
fn the_command_can_open_the_offer_after_copy_cleared_the_highlight(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    view.update(cx, |view, _| view.config.keep_selection_after_copy = false);
    offer(&view, cx);
    assert!(view.read_with(cx, |view, _| view.selection.is_none()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.annotate_selection(window, cx);
            assert!(view.terminal_note_focused(window, cx));
            assert_eq!(
                view.terminal_notes.composer.as_ref().unwrap().quote,
                "failed"
            );
        })
    });
}
