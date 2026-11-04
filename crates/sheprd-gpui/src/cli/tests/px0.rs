use super::*;

#[test]
fn px0_is_a_control_command_with_shared_flags() {
    assert_eq!(
        LaunchOptions::parse(["px0", "open", "--json", "--window", "7"])
            .unwrap()
            .mode,
        LaunchMode::Control(control::Invocation {
            command: control::ControlCommand::Px0,
            json: true,
            window: Some(7),
        })
    );
    assert_eq!(
        LaunchOptions::parse(["px0", "--help"]).unwrap().mode,
        LaunchMode::Control(control::Invocation {
            command: control::ControlCommand::Help(Some("px0")),
            json: false,
            window: None,
        })
    );
    for args in [["px0", "bad"], ["px0", "--unknown"]] {
        assert!(LaunchOptions::parse(args).is_err());
    }
}
