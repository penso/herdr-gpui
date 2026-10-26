use super::*;

#[test]
fn the_composer_takes_pasted_and_picked_pictures() {
    let view = View {
        transcript: Transcript::new("Claude"),
        connected: true,
    };
    let html = chat(&workspace(vec![]), &row("p1"), &view, 0).into_string();
    let textarea = html
        .split("<textarea")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .unwrap();
    // app.js turns a pasted screenshot into an attachment; a bare textarea
    // would drop it.
    assert!(textarea.contains(r#"x-on:paste="paste""#), "{textarea}");
    assert!(html.contains(r#"x-on:change="attach""#));
}

/// The opening tag of the composer button labelled `label`.
fn button<'a>(html: &'a str, label: &str) -> &'a str {
    let label = format!(r#"aria-label="{label}""#);
    let at = html.find(&label).unwrap();
    let start = html[..at].rfind("<button").unwrap();
    let end = at + html[at..].find('>').unwrap();
    &html[start..end]
}

#[test]
fn one_button_stops_or_sends() {
    let mut view = View {
        transcript: Transcript::new("Claude"),
        connected: true,
    };
    let idle = chat(&workspace(vec![]), &row("p1"), &view, 0).into_string();
    // Idle with an empty draft: only a disabled Send shows.
    let stop = button(&idle, "Stop");
    let send = button(&idle, "Send");
    assert!(stop.contains("display:none"), "{stop}");
    assert!(
        !send.contains("display:none") && send.contains("disabled"),
        "{send}"
    );
    assert!(!idle.contains("data-working"));

    view.transcript.status = Status::Working;
    let working = chat(&workspace(vec![]), &row("p1"), &view, 0).into_string();
    // Working with an empty draft: only Stop shows, until the user types.
    let stop = button(&working, "Stop");
    let send = button(&working, "Send");
    assert!(!stop.contains("display:none"), "{stop}");
    assert!(send.contains("display:none"), "{send}");
    assert!(send.contains(r#"x-show="sendShown""#), "{send}");
    assert!(working.contains("data-working"));
}

#[test]
fn the_draft_field_can_shrink_so_the_button_fits() {
    let view = View {
        transcript: Transcript::new("Claude"),
        connected: true,
    };
    let html = chat(&workspace(vec![]), &row("p1"), &view, 0).into_string();
    let textarea = html.split("<textarea").nth(1).unwrap();
    // Without min-width 0 a textarea's intrinsic width pushes the button
    // past a phone's edge.
    assert!(textarea.split('>').next().unwrap().contains("min-w-0"));
}
