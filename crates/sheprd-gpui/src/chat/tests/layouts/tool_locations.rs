use super::*;
use crate::chat::selection::{Cell, Region, Selection};
use sheprd_chat_protocol::{Tool, ToolKind, ToolStatus};

const PATH: &str = "src/visible-location.rs";

fn tool() -> ServerMessage {
    snapshot(
        cursor(1, 0),
        vec![Entry {
            id: EntryId(1),
            kind: EntryKind::Tool(Tool {
                started_ms: None,
                parent: None,
                call_id: "read".into(),
                content: Vec::new(),
                kind: ToolKind::Read,
                locations: vec![PATH.into()],
                status: ToolStatus::Completed,
                title: "Inspect source".into(),
            }),
        }],
    )
}

fn check_location(view: &Entity<SheprdWindow>, visible: bool, cx: &VisualTestContext) {
    view.read_with(cx, |view, _| {
        let pane = view.chats.pane("w1:p1").unwrap();
        let text = pane.prepared.text();
        let location = text.key(EntryId(1), Region::ToolLocations, Cell::default());
        assert_eq!(location.is_some(), visible);
        if let Some(key) = location {
            assert_eq!(text.segment(key).unwrap().text(), PATH);
            assert!(pane.text_geometry.position(key, 0).is_some());
        }
        let mut selection = Selection::default();
        selection.select_all(text);
        let copied = selection.text(text).unwrap();
        assert!(copied.contains("Inspect source"));
        assert_eq!(copied.contains(PATH), visible);
    });
}

#[gpui::test]
fn copy_tracks_folded_locations_when_layout_changes(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, tool(), cx);
    check_location(&view, true, cx);
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        pane.selection.select_all(pane.prepared.text());
    });
    switch(&view, ChatLayoutMode::IA, cx);
    poll(&view, &program, cx);
    check_location(&view, false, cx);
    assert!(view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().selection.is_empty()
    }));

    for mode in ChatLayoutMode::ALL {
        switch(&view, *mode, cx);
        poll(&view, &program, cx);
        check_location(&view, *mode != ChatLayoutMode::IA, cx);
    }
}

#[gpui::test]
fn a_new_ia_chat_copies_locations_only_while_expanded(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    switch(&view, ChatLayoutMode::IA, cx);
    let program = Program::new();
    let _stream = attach_with(&view, &program, tool(), cx);
    check_location(&view, false, cx);
    for visible in [true, false] {
        view.update(cx, |view, _| {
            view.chats
                .pane_mut("w1:p1")
                .unwrap()
                .toggle_expanded(EntryId(1));
        });
        poll(&view, &program, cx);
        check_location(&view, visible, cx);
    }
}
