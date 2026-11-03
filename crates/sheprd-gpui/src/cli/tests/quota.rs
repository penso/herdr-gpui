use super::*;

#[test]
fn quota_accepts_shared_options_and_rejects_extra_arguments() {
    let parsed = LaunchOptions::parse(["pr", "quota", "--json", "--window", "7"]).unwrap();
    assert_eq!(
        parsed.mode,
        LaunchMode::Control(control::Invocation {
            command: control::ControlCommand::Pr(control::PrCommand::Quota),
            json: true,
            window: Some(7),
        })
    );
    assert!(LaunchOptions::parse(["pr", "quota", "extra"]).is_err());
}
