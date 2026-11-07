use super::*;

#[core::prelude::v1::test]
fn the_answer_names_known_agents_in_table_order() {
    assert_eq!(
        parse(b"opencode\nteleporter\n codex \n\nopencode\n"),
        [Agent::Codex, Agent::Opencode]
    );
    assert!(parse(b"").is_empty());
}

#[core::prelude::v1::test]
fn a_host_is_asked_again_only_once_its_answer_is_old() {
    let mut harnesses = Harnesses::default();
    let now = Instant::now();
    assert!(harnesses.begin("e1", LOCAL_REFRESH, now));
    assert!(
        !harnesses.begin("e1", LOCAL_REFRESH, now),
        "one ask at a time"
    );
    harnesses.finish("e1", Some(vec![Agent::Gemini]));
    assert_eq!(harnesses.agents("e1"), Some(&[Agent::Gemini][..]));
    assert!(!harnesses.begin("e1", LOCAL_REFRESH, now + Duration::from_secs(1)));
    assert!(harnesses.begin("e1", LOCAL_REFRESH, now + LOCAL_REFRESH));
    // A failed ask keeps the last answer.
    harnesses.finish("e1", None);
    assert_eq!(harnesses.agents("e1"), Some(&[Agent::Gemini][..]));
    assert_eq!(harnesses.agents("e2"), None);
}
