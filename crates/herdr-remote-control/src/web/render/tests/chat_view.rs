use super::*;

fn view(transcript: Transcript) -> View {
    View {
        transcript,
        connected: true,
    }
}

#[test]
fn the_latest_turn_is_in_focus_and_earlier_ones_fold() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![
        entry(
            1,
            EntryKind::User {
                text: "first".into(),
            },
        ),
        entry(
            2,
            EntryKind::Agent {
                text: "old answer".into(),
            },
        ),
        entry(
            3,
            EntryKind::User {
                text: "second prompt".into(),
            },
        ),
        entry(
            4,
            EntryKind::Agent {
                text: "**new** answer".into(),
            },
        ),
    ];
    let html = chat(&workspace(vec![]), &row("p1"), &view(transcript.clone()), 0).into_string();
    assert!(html.contains("second prompt"));
    assert!(html.contains("<strong>new</strong>"));
    assert!(!html.contains("old answer"));
    assert!(html.contains("earlier turn"));
    let history = chat_history(&view(transcript)).into_string();
    assert!(history.contains("old answer"));
    assert!(!history.contains("second prompt"));
}

#[test]
fn a_question_is_a_sheet_with_allowing_first_and_always_apart() {
    let mut transcript = Transcript::new("Claude");
    transcript.permissions = vec![question("Run just ci?"), question("Edit Cargo.toml?")];
    let html = chat(&workspace(vec![]), &row("p1"), &view(transcript), 0).into_string();
    let sheet = &html[html.find("chat-sheet").unwrap()..];
    let once = sheet.find("Allow once").unwrap();
    let deny = sheet.find("Deny").unwrap();
    let always = sheet.find("Always allow").unwrap();
    assert!(once < deny && deny < always);
    assert!(sheet.contains("btn-subtle"));
    assert!(sheet.contains("1 more waiting"));
    assert!(sheet.contains(r#"&quot;permission&quot;:7"#));
}

#[test]
fn patches_cover_every_region_the_stream_replaces() {
    let patches = chat_patches(
        &workspace(vec![]),
        &row("p1"),
        &view(Transcript::new("Claude")),
        0,
    );
    let ids: Vec<_> = patches.iter().map(|(id, _)| *id).collect();
    for id in [
        "chat-head",
        "chat-turn",
        "chat-sheet",
        "chat-settings",
        "chat-composer",
    ] {
        assert!(ids.contains(&id));
        let (_, markup) = patches
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .unwrap();
        assert!(
            markup
                .clone()
                .into_string()
                .contains(&format!(r#"id="{id}""#))
        );
    }
}
