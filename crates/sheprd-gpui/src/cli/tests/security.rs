use super::*;
use control::{ControlCommand, Invocation, SecurityCommand};

#[test]
fn security_takes_the_shared_options() {
    let parsed = LaunchOptions::parse(["security", "--json", "--window", "3"]).unwrap();
    let LaunchMode::Control(invocation) = parsed.mode else {
        panic!("security parsed as {:?}", parsed.mode);
    };
    assert_eq!(
        invocation,
        Invocation {
            command: ControlCommand::Security(SecurityCommand::List),
            json: true,
            window: Some(3),
        }
    );
}

#[test]
fn security_open_names_a_repository_and_an_item() {
    let command = |args: &[&str]| match LaunchOptions::parse(args.iter().copied()) {
        Ok(LaunchOptions {
            mode: LaunchMode::Control(invocation),
            ..
        }) => Ok(invocation.command),
        Ok(other) => panic!("security parsed as {:?}", other.mode),
        Err(error) => Err(error),
    };
    assert_eq!(
        command(&["security", "open", "sheprd-labs/sheprd", "42"]).unwrap(),
        ControlCommand::Security(SecurityCommand::Open {
            repository: "sheprd-labs/sheprd".into(),
            item: "42".into(),
        })
    );
    assert_eq!(
        command(&["security", "list"]).unwrap(),
        ControlCommand::Security(SecurityCommand::List)
    );
    assert!(command(&["security", "open", "sheprd-labs/sheprd"]).is_err());
    assert!(command(&["security", "dismiss", "sheprd-labs/sheprd", "42"]).is_err());
}
