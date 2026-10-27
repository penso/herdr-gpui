use super::*;

#[gpui::test]
fn terminal_keeps_its_reading_role_and_offers_insertion_only_with_input_focus(
    cx: &mut TestAppContext,
) {
    let peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(&peer, view, window, cx);
            let terminal = node(view, window, cx);
            assert_eq!(terminal.role(), Role::Terminal);
            assert_eq!(terminal.label(), Some("Terminal"));
            assert!(terminal.supports_action(Action::Focus));
            assert!(terminal.supports_action(Action::SetValue));
            // This is the capability AccessKit's macOS adapter uses to expose
            // setAccessibilityValue: even for the read-only Terminal role.
            assert!(!terminal.supports_action(Action::SetTextSelection));

            let field = cx.focus_handle();
            window.focus(&field, cx);
            assert!(!node(view, window, cx).supports_action(Action::SetValue));
            window.focus(&view.focus, cx);
            view.menu.page = Some(crate::menu::Page::Palette);
            assert!(!node(view, window, cx).supports_action(Action::SetValue));
            view.menu.page = None;
            view.marked = "候補".into();
            assert!(!node(view, window, cx).supports_action(Action::SetValue));
            view.marked.clear();
            view.live.supports_copy_motion = true;
            view.enter_copy_mode(window, cx);
            assert!(view.copy_mode_active());
            assert!(!node(view, window, cx).supports_action(Action::SetValue));
            view.leave_copy_mode(cx);
            view.endpoints[0].connection.handle = None;
            assert!(!node(view, window, cx).supports_action(Action::SetValue));
        })
    });
}

#[gpui::test]
fn insertion_pastes_unicode_and_multiline_text_into_the_focused_pane(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(&peer, view, window, cx);
            let target = view.accessible_insertion_target(window).unwrap();
            view.insert_accessible_text(
                &target,
                Some(&value("hello 世界\nsecond line")),
                window,
                cx,
            );
        })
    });
    assert_eq!(next_input(&mut peer), pane_input("hello 世界\nsecond line"));
    assert_no_input(&mut peer);
}

#[gpui::test]
fn a_readable_selection_in_another_pane_does_not_redirect_or_replace_input(
    cx: &mut TestAppContext,
) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(&peer, view, window, cx);
            let mut surface = crate::window::selection::tests::surface(&["left right"], 10);
            let old = view.live.surface.as_ref().unwrap();
            surface.boot_id = old.boot_id.clone();
            surface.projection_revision = old.projection_revision;
            surface.panes[0].pane_id = "w1:p1".into();
            surface.panes[0].rect.width = 5;
            surface.panes[0].inner_rect.width = 5;
            let mut right = surface.panes[0].clone();
            right.pane_id = "right".into();
            right.focused = false;
            right.rect.x = 5;
            right.inner_rect.x = 5;
            surface.panes.push(right);
            let height = view.config.terminal.line_height();
            let mut selection =
                crate::terminal::Selection::begin(&surface, 51., 1., view.cell_width, height, 1)
                    .unwrap();
            selection.extend(&surface, 99., 1., view.cell_width, height);
            selection.release();
            assert_eq!(
                selection.text(&surface, view.cell_width, height).unwrap(),
                "right"
            );
            let output = surface.frame.clone();
            view.live.surface = Some(Arc::new(surface));
            view.options.surface_size.cols = 10;
            view.options.surface_size.rows = 1;
            view.selection = Some(selection);
            let target = view.accessible_insertion_target(window).unwrap();
            assert_eq!(target.input, InputTarget::Pane("w1:p1".into()));
            view.insert_accessible_text(&target, Some(&value("inserted")), window, cx);
            assert!(
                view.selection.is_none(),
                "insertion clears the highlight like typing"
            );
            assert_eq!(view.live.surface.as_ref().unwrap().frame, output);
        })
    });
    assert_eq!(next_input(&mut peer), pane_input("inserted"));
    assert_no_input(&mut peer);
}

#[gpui::test]
fn popup_insertion_rejects_the_old_pane_target_and_uses_popup_semantic_input(
    cx: &mut TestAppContext,
) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(&peer, view, window, cx);
            let pane = view.accessible_insertion_target(window).unwrap();
            popup(view);
            view.insert_accessible_text(&pane, Some(&value("stale")), window, cx);
            let target = view.accessible_insertion_target(window).unwrap();
            assert_eq!(target.input, InputTarget::Popup("popup".into()));
            assert!(node(view, window, cx).supports_action(Action::SetValue));
            view.insert_accessible_text(&target, Some(&value("dictation")), window, cx);
            Arc::make_mut(view.live.surface.as_mut().unwrap()).popup = None;
            view.insert_accessible_text(&target, Some(&value("closed popup")), window, cx);
        })
    });
    assert_eq!(
        next_input(&mut peer),
        ClientMessage::ClientShellPopupInput {
            terminal_id: "popup".into(),
            events: vec![ClientPaneInputEvent::Paste("dictation".into())],
        }
    );
    assert_no_input(&mut peer);
}

#[gpui::test]
fn delayed_actions_cannot_cross_focus_or_connection_changes(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(&peer, view, window, cx);
            let target = view.accessible_insertion_target(window).unwrap();
            let data = value("stale");
            window.blur(cx);
            view.insert_accessible_text(&target, Some(&data), window, cx);
            window.focus(&view.focus, cx);
            view.menu.page = Some(crate::menu::Page::Palette);
            view.insert_accessible_text(&target, Some(&data), window, cx);
            view.menu.page = None;
            view.live.supports_copy_motion = true;
            view.enter_copy_mode(window, cx);
            view.insert_accessible_text(&target, Some(&data), window, cx);
            view.leave_copy_mode(cx);
            view.options.surface_size.cols += 1;
            view.insert_accessible_text(&target, Some(&data), window, cx);
            view.options.surface_size.cols -= 1;
            view.selection_epoch += 1;
            view.insert_accessible_text(&target, Some(&data), window, cx);
            view.selection_epoch -= 1;
            view.endpoints[0].generation += 1;
            view.insert_accessible_text(&target, Some(&data), window, cx);
            view.endpoints[0].generation -= 1;
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_pane_id =
                Some("other".into());
            view.insert_accessible_text(&target, Some(&data), window, cx);
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_pane_id =
                Some("w1:p1".into());
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).boot_id = "new-boot".into();
            Arc::make_mut(view.live.surface.as_mut().unwrap()).boot_id = "new-boot".into();
            view.insert_accessible_text(&target, Some(&data), window, cx);
        })
    });
    assert_no_input(&mut peer);
}

#[gpui::test]
fn accessibility_neither_commits_nor_discards_an_ime_composition(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(&peer, view, window, cx);
            let target = view.accessible_insertion_target(window).unwrap();
            view.replace_and_mark_text_in_range(None, "候補", Some(0..1), window, cx);
            view.insert_accessible_text(&target, Some(&value("dictation")), window, cx);
            assert_eq!(view.marked, "候補");
            assert_eq!(view.marked_selection, Some(0..1));
            view.replace_text_in_range(None, "候補", window, cx);
            assert!(view.marked.is_empty());
        })
    });
    assert_eq!(
        next_input(&mut peer),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::TextCommit("候補".into())],
        }
    );
    assert_no_input(&mut peer);
}

#[gpui::test]
fn empty_wrongly_typed_and_oversized_values_send_nothing(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(&peer, view, window, cx);
            let target = view.accessible_insertion_target(window).unwrap();
            for data in [None, Some(value("")), Some(ActionData::NumericValue(42.))] {
                view.insert_accessible_text(&target, data.as_ref(), window, cx);
            }
            assert!(view.local_error.is_none());
            view.insert_accessible_text(
                &target,
                Some(&value(&"x".repeat(MAX_FRAME_SIZE + 1))),
                window,
                cx,
            );
            assert_eq!(
                view.local_error.as_deref(),
                Some("Accessibility input not sent: frame exceeds limit")
            );
        })
    });
    assert_no_input(&mut peer);
}
