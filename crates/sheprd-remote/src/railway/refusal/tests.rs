#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn links_are_hidden_wherever_they_sit() {
    let hidden =
        hide_links(r#"{"status":"trial_ready","human_claim_url":"https://claim.example/secret"}"#);
    assert!(!hidden.contains("claim.example") && !hidden.contains("secret"));
    assert!(hidden.contains("[link hidden]") && hidden.contains("trial_ready"));
    // An `https` link before an `http` one on a compact line is hidden too.
    let mixed = hide_links(
        r#"{"human_claim_url":"https://claim.example/secret","preview_url":"http://p.example"}"#,
    );
    assert!(!mixed.contains("claim.example") && !mixed.contains("secret"));
    assert_eq!(
        hide_links("see http://a.b/c now\nnext"),
        "see [link hidden] now\nnext"
    );
}

#[test]
fn only_a_headless_refusal_asks_for_a_terminal() {
    let headless = r#"{"status":"refused","human_signup_url":"https://s.example/x","description":"Headless terminals are not allowed for anonymous trials. Sign up to script against Railway."}"#;
    assert_eq!(refusal(headless), Refusal::NeedsTerminal);
    assert_eq!(
        refusal(&format!("noise\n{headless}\n")),
        Refusal::NeedsTerminal
    );
    for output in [
        String::new(),
        "Anonymous visitors are limited. Sign up to keep building.".into(),
        r#"{"status":"refused","description":"Anonymous visitors are limited."}"#.into(),
        r#"{"status":"trial_ready","description":"headless"}"#.into(),
        "Headless terminals are not allowed".into(),
    ] {
        assert_eq!(refusal(&output), Refusal::Other, "{output}");
    }
}
