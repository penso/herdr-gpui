use super::*;

/// The middle of cell `col` of row `row`.
fn cell(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    col: u16,
    row: u16,
) -> Point<Pixels> {
    view.read_with(cx, |view, _| {
        let height = view.config.terminal.line_height();
        view.bounds.origin
            + point(
                px((f32::from(col) + 0.5) * view.cell_width),
                px((f32::from(row) + 0.5) * height),
            )
    })
}

#[gpui::test]
fn a_wrapped_path_is_underlined_on_every_row_and_opens_from_any(cx: &mut gpui::TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let folder = "some/deeply/nested/folder/with/many/levels";
    std::fs::create_dir_all(root.path().join(folder)).unwrap();
    std::fs::write(root.path().join(folder).join("notes.txt"), "notes").unwrap();
    // In the fixture's 60 columns the path starts at column 15 and wraps
    // after `levels/no`, going on for seven columns of the second row.
    let (view, cx) = window(
        &format!("the file is at {folder}/notes.txt now"),
        root.path(),
        cx,
    );
    let head = cell(&view, cx, 20, 0);
    let tail = cell(&view, cx, 3, 1);

    // Held over the second row, the link modifier has the whole path looked
    // for on disk; found, every row of it is underlined.
    cx.simulate_mouse_move(tail, None, Modifiers::secondary_key());
    cx.run_until_parked();
    let rows = vec![(0, 15..60), (1, 0..7)];
    view.read_with(cx, |view, _| {
        let link = view.hovered_local_link().unwrap();
        assert_eq!(link.rows().collect::<Vec<_>>(), rows);
        for position in [head, tail] {
            assert!(view.terminal_link_hovered(position, Modifiers::secondary_key()));
        }
    });
    // Moving onto the first row keeps the same link.
    cx.simulate_mouse_move(head, None, Modifiers::secondary_key());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let link = view.hovered_local_link().unwrap();
        assert_eq!(link.rows().collect::<Vec<_>>(), rows);
    });

    // A click on the second row's part opens the whole path.
    let alt = Modifiers {
        alt: true,
        ..Modifiers::secondary_key()
    };
    modifier_click(cx, tail, alt);
    let real = real_path(&root.path().join(folder).join("notes.txt")).unwrap();
    let expected = url::Url::from_file_path(real).unwrap();
    assert_eq!(cx.opened_url().as_deref(), Some(expected.as_str()));
}

#[gpui::test]
fn a_path_an_agent_wrapped_under_an_indent_opens_from_either_line(cx: &mut gpui::TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let folder = "evidence/citynet2/shots-before";
    std::fs::create_dir_all(root.path().join(folder)).unwrap();
    std::fs::write(
        root.path().join(folder).join("tile77-street2-round1.png"),
        "png",
    )
    .unwrap();
    // A list item an agent wrapped itself in the fixture's 60 columns: the
    // path fills the first row and goes on under the item's text.
    let item = format!("{:<25}{folder}/tile", "  - round 1:");
    let rest = format!("{:<60}", "    77-street2-round1.png");
    let (view, cx) = window(&format!("{item}{rest}"), root.path(), cx);
    let head = cell(&view, cx, 30, 0);
    let tail = cell(&view, cx, 6, 1);
    let indent = cell(&view, cx, 1, 1);

    cx.simulate_mouse_move(tail, None, Modifiers::secondary_key());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let link = view.hovered_local_link().unwrap();
        assert_eq!(
            link.rows().collect::<Vec<_>>(),
            vec![(0, 25..60), (1, 4..25)]
        );
        assert!(view.terminal_link_hovered(head, Modifiers::secondary_key()));
        // The indent before the rest is not part of the link.
        assert!(!view.terminal_link_hovered(indent, Modifiers::secondary_key()));
    });

    let alt = Modifiers {
        alt: true,
        ..Modifiers::secondary_key()
    };
    modifier_click(cx, tail, alt);
    let real = real_path(&root.path().join(folder).join("tile77-street2-round1.png")).unwrap();
    let expected = url::Url::from_file_path(real).unwrap();
    assert_eq!(cx.opened_url().as_deref(), Some(expected.as_str()));
}
