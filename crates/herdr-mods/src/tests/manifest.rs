use super::*;

#[test]
fn a_full_manifest_parses() {
    let manifest = Manifest::parse(
        "name = \"pr-badge\"\napi = 1\nplaces = [\"status_bar\", \"sidebar\", \"status_bar\"]\n\
         commands = [{ id = \"refresh\", title = \"Refresh PR badge\" }]\n\
         [activity]\ntitle = \"Deploys\"\nicon = \"icon.svg\"\n\
         [needs]\nexec = [\"gh\"]\n",
    )
    .unwrap();
    assert_eq!(manifest.name, "pr-badge");
    assert_eq!(manifest.places, [Place::StatusBar, Place::Sidebar]);
    assert_eq!(manifest.commands[0].id, "refresh");
    assert_eq!(manifest.activity.unwrap().icon, "icon.svg");
    assert_eq!(manifest.needs.exec, ["gh"]);
    assert!(!manifest.needs.is_empty());
}

#[test]
fn a_minimal_manifest_needs_nothing() {
    let manifest = Manifest::parse(MANIFEST).unwrap();
    assert!(manifest.places.is_empty());
    assert!(manifest.needs.is_empty());
}

fn problem(text: &str) -> (String, Problem) {
    match Manifest::parse(text).unwrap_err() {
        Error::Manifest { at, problem } => (at, problem),
        other => panic!("expected a manifest error, got {other:?}"),
    }
}

#[test]
fn unknown_keys_are_refused_by_toml() {
    let error =
        Manifest::parse("name = \"demo\"\napi = 1\nplaces = []\nversion = 2\n").unwrap_err();
    assert!(matches!(error, Error::Toml(_)), "{error:?}");
}

#[test]
fn another_api_is_refused() {
    assert_eq!(problem("name = \"demo\"\napi = 2\n").0, "api");
}

#[test]
fn names_are_plain_identifiers() {
    for name in ["", "Demo", "1demo", "de mo", "demo-", "de--mo", "../x"] {
        let (at, _) = problem(&format!("name = \"{name}\"\napi = 1\n"));
        assert_eq!(at, "name", "{name:?}");
    }
    let long = "a".repeat(41);
    assert_eq!(
        problem(&format!("name = \"{long}\"\napi = 1\n")).1,
        Problem::TooLong { limit: 40 }
    );
}

#[test]
fn unknown_places_list_the_known_ones() {
    let (at, problem) = problem("name = \"demo\"\napi = 1\nplaces = [\"titlebar\"]\n");
    assert_eq!(at, "places[0]");
    assert_eq!(problem, Problem::OneOf(Place::NAMES));
}

#[test]
fn command_ids_are_unique_identifiers() {
    let (at, _) = problem(
        "name = \"demo\"\napi = 1\ncommands = [{ id = \"a\", title = \"A\" }, { id = \"a\", title = \"B\" }]\n",
    );
    assert_eq!(at, "commands[1].id");
    let (at, problem) =
        problem("name = \"demo\"\napi = 1\ncommands = [{ id = \"a\", title = \"A\\u0007\" }]\n");
    assert_eq!(
        (at.as_str(), problem),
        ("commands[0].title", Problem::Control)
    );
}

#[test]
fn the_activity_icon_is_an_svg_in_the_folder() {
    for icon in ["icon.png", "../icon.svg", "sub/icon.svg"] {
        let (at, _) = problem(&format!(
            "name = \"demo\"\napi = 1\n[activity]\ntitle = \"Demo\"\nicon = \"{icon}\"\n"
        ));
        assert_eq!(at, "activity.icon", "{icon}");
    }
}

#[test]
fn needs_are_bounded() {
    let many = (0..17)
        .map(|n| format!("\"c{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let (at, problem) = problem(&format!(
        "name = \"demo\"\napi = 1\n[needs]\nexec = [{many}]\n"
    ));
    assert_eq!(
        (at.as_str(), problem),
        ("needs.exec", Problem::Entries { limit: 16 })
    );
}

#[test]
fn a_panel_section_takes_a_title_and_order() {
    let manifest = Manifest::parse(
        "name = \"todo\"\napi = 1\nplaces = [\"panel\"]\n[panel]\ntitle = \"Todo\"\norder = 400\n",
    )
    .unwrap();
    assert_eq!(manifest.places, [Place::Panel]);
    assert_eq!(
        manifest.panel,
        Panel {
            title: Some("Todo".into()),
            order: 400
        }
    );
    let default = Manifest::parse("name = \"todo\"\napi = 1\nplaces = [\"panel\"]\n").unwrap();
    assert_eq!(default.panel.title, None);
}

#[test]
fn a_panel_table_needs_the_panel_place_and_a_bounded_order() {
    assert_eq!(
        problem("name = \"todo\"\napi = 1\n[panel]\ntitle = \"Todo\"\n").0,
        "panel"
    );
    assert_eq!(
        problem("name = \"todo\"\napi = 1\nplaces = [\"panel\"]\n[panel]\norder = 99999\n").0,
        "panel.order"
    );
}
