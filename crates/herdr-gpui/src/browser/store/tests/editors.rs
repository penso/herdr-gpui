use super::*;

#[test]
fn editor_identity_preserves_the_saved_flat_names() {
    for (name, place) in [
        ("group", Place::Group),
        ("code", Place::Editor(Editor::VsCode)),
        ("px0", Place::Editor(Editor::Px0)),
    ] {
        let json = format!("\"{name}\"");
        assert_eq!(serde_json::to_string(&place).unwrap(), json);
        assert_eq!(serde_json::from_str::<Place>(&json).unwrap(), place);
    }
    assert_eq!(
        serde_json::from_str::<Place>(r#""code_group""#).unwrap(),
        Place::Editor(Editor::VsCode)
    );
    assert!(serde_json::from_str::<Place>(r#""unknown-editor""#).is_err());
}

#[test]
fn every_editor_reuses_navigation_and_reopens_without_duplicates() {
    for editor in [Editor::Px0, Editor::VsCode] {
        let scope = Scope::endpoint("local");
        let mut store = Store::default();
        let first = store
            .open_editor_tab(scope.clone(), "w", editor, None)
            .unwrap();
        assert_eq!(store.get(first).unwrap().title, editor.name());
        let visited = url("http://localhost:1234/workspace/file");
        store.visited(first, visited.clone(), Some("Editing"));
        assert_eq!(
            store.open_editor_tab(scope.clone(), "w", editor, url("http://localhost:9999/")),
            Some(first)
        );
        assert_eq!(store.get(first).unwrap().location, visited);
        let closed = store.close(first).unwrap();
        let replacement = store
            .open_editor_tab(scope.clone(), "w", editor, None)
            .unwrap();
        assert_eq!(store.reopen_page(closed), Some(replacement));
        assert_eq!(
            store
                .in_workspace(&scope, "w")
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            [replacement]
        );
    }
}

#[test]
fn a_waiting_editor_tab_keeps_its_name_and_a_blank_tab_is_new() {
    let scope = Scope::endpoint("local");
    let mut store = Store::default();
    for editor in [Editor::Px0, Editor::VsCode] {
        let id = store
            .open_editor_tab(scope.clone(), "w", editor, None)
            .unwrap();
        assert_eq!(store.get(id).unwrap().label(), editor.name());
    }
    let blank = store.open(scope, "w", None, None).unwrap();
    assert_eq!(
        store.get(blank).unwrap().label(),
        crate::i18n::message("browser-tab-new", &[])
    );
}

#[test]
fn a_terminal_tab_shows_its_programs_title_and_otherwise_terminal() {
    let terminal = Location::CoreTerminal {
        panes: crate::core_terminals::panes::Panes::single(
            herdr_backend::TerminalId::try_from("t1".to_owned()).unwrap(),
        ),
        observe: false,
    };
    let mut store = Store::default();
    let id = store
        .open(Scope::endpoint("local"), "w", Some(terminal), None)
        .unwrap();
    let drawn = crate::i18n::message("browser-tab-terminal", &[]);
    assert_eq!(store.get(id).unwrap().label(), drawn);
    store.visited(id, None, Some("vim notes.md"));
    assert_eq!(store.get(id).unwrap().label(), "vim notes.md");
}
