use super::*;

#[test]
fn pair_accepts_one_address_and_keeps_shared_options() {
    let invocation = parse(&[
        "remote",
        "pair",
        "--address",
        "https://100.64.0.3:7443",
        "--json",
        "--window",
        "3",
    ])
    .unwrap();
    assert_eq!(
        invocation.command,
        ControlCommand::Remote(RemoteCommand::Pair {
            address: Some("https://100.64.0.3:7443".into()),
        })
    );
    assert!(invocation.json);
    assert_eq!(invocation.window, Some(3));
    assert!(matches!(
        parse(&["remote", "pair", "--address"]),
        Err(CliError::MissingValue("--address"))
    ));
    assert!(matches!(
        parse(&["remote", "pair", "--address", "a", "--address", "b"]),
        Err(CliError::DuplicateOption("--address"))
    ));
    assert!(parse(&["remote", "status", "--address", "a"]).is_err());
}
