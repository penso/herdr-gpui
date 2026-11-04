use super::*;

#[test]
fn script_failures_keep_step_and_source() {
    let error = script(Step::StartAgent)(sheprd_client::Error::ScriptTimeout);
    assert!(matches!(
        error,
        Error::Script {
            step: Step::StartAgent,
            source: sheprd_client::Error::ScriptTimeout
        }
    ));
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(
        error.to_string(),
        "starting the agent failed: host script made no progress before its deadline"
    );
    assert!(matches!(
        script(Step::Prompt)(sheprd_client::Error::ScriptCancelled),
        Error::Cancelled
    ));
}
