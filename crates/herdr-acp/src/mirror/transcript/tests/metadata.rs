use super::*;
use herdr_chat_protocol::{OptionValue, Status};

#[test]
fn codex_reports_latest_model_and_context_without_claiming_liveness() {
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[
            json!({"type":"turn_context","payload":{"model":"first-model","effort":"high"}}),
            json!({"type":"event_msg","payload":{"type":"token_count","info":{
                "model_context_window":200000,
                "last_token_usage":{"total_tokens":50000},
                "total_token_usage":{"total_tokens":900000}
            }}}),
            json!({"type":"turn_context","payload":{"model":"latest-model","effort":"medium"}}),
            json!({"type":"event_msg","payload":{"type":"token_count","info":null}}),
        ]),
        100,
    );
    let model = transcript.option(OptionCategory::Model).unwrap();
    assert_eq!(model.current_name(), "latest-model");
    assert!(!model.accepts(&OptionValue::Choice {
        id: "latest-model".into()
    }));
    assert_eq!(
        transcript
            .option(OptionCategory::ThoughtLevel)
            .unwrap()
            .current_name(),
        "medium"
    );
    assert_eq!(
        transcript.usage,
        Some(Usage {
            cost: None,
            size: 200000,
            used: 50000
        })
    );
    assert_eq!(
        transcript.status,
        Status::Starting,
        "logs cannot prove the live process is idle"
    );
    assert!(transcript.turn.is_none());
    assert!(transcript.read_only);
}

#[test]
fn invalid_context_counts_do_not_invent_usage() {
    for (size, used) in [
        (json!(0), json!(5)),
        (json!(-1), json!(5)),
        (json!(200), json!("5")),
    ] {
        let transcript = parse(
            TranscriptAgent::Codex,
            &lines(&[
                json!({"type":"event_msg","payload":{"type":"token_count","info":{
                    "model_context_window":size,"last_token_usage":{"total_tokens":used}
                }}}),
            ]),
            0,
        );
        assert!(transcript.usage.is_none());
    }
}

#[test]
fn claude_reports_only_the_primary_assistants_bounded_model() {
    let model = "α".repeat(200);
    let transcript = parse(
        TranscriptAgent::Claude,
        &lines(&[
            json!({"type":"assistant","message":{"model":model,"content":"answer","usage":{"input_tokens":1000}}}),
            json!({"type":"assistant","isSidechain":true,"message":{"model":"side-model","content":"other"}}),
            json!({"type":"user","message":{"model":"not-a-model","content":"question"}}),
        ]),
        0,
    );
    assert_eq!(
        transcript
            .option(OptionCategory::Model)
            .unwrap()
            .current_name(),
        "α".repeat(128)
    );
    assert!(
        transcript.usage.is_none(),
        "no context capacity was reported"
    );
}

#[test]
fn a_codex_header_names_the_same_session_the_observer_verifies() {
    let payload = json!({"id":"rollout","session_id":"child","cwd":"/work"});
    let bytes = lines(&[json!({"type":"session_meta","payload":payload})]);
    let transcript = parse(TranscriptAgent::Codex, &bytes, 0);
    assert_eq!(transcript.session.unwrap().id, "rollout");
    assert_eq!(codex_session(&payload), Some("rollout"));
}
