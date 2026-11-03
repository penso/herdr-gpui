use super::*;
use control::{BatchCommand, CloudCommand, ControlCommand, Invocation};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

fn batch(args: &[&str]) -> BatchCommand {
    match parse(args).unwrap().command {
        ControlCommand::Cloud(CloudCommand::Batch(command)) => command,
        command => panic!("{args:?} parsed as {command:?}"),
    }
}

#[test]
fn cloud_destroy_names_devices_or_a_batch() {
    assert_eq!(
        batch(&["cloud", "destroy", "coder:w1", "Build box", "--json"]),
        BatchCommand::Destroy {
            devices: vec!["coder:w1".into(), "Build box".into()],
            batch: None,
        }
    );
    assert_eq!(
        batch(&["cloud", "destroy", "--batch", "3"]),
        BatchCommand::Destroy {
            devices: Vec::new(),
            batch: Some(3),
        }
    );
    assert_eq!(
        parse(&["cloud", "destroy"]).unwrap_err(),
        CliError::MissingArgument {
            command: "cloud destroy",
            what: "ID or --batch BATCH"
        }
    );
    for invalid in [["--batch", "0"], ["--batch", "x"]] {
        let args: Vec<_> = ["cloud", "destroy"].into_iter().chain(invalid).collect();
        assert_eq!(parse(&args).unwrap_err(), CliError::InvalidValue("--batch"));
    }
    let many: Vec<String> = (0..65).map(|index| format!("d{index}")).collect();
    let args: Vec<&str> = ["cloud", "destroy"]
        .into_iter()
        .chain(many.iter().map(String::as_str))
        .collect();
    assert_eq!(parse(&args).unwrap_err(), CliError::InvalidValue("ID"));
}

#[test]
fn cloud_wait_follows_one_batch_for_a_bounded_time() {
    assert_eq!(
        batch(&["cloud", "wait", "3"]),
        BatchCommand::Wait {
            batch: 3,
            timeout: 60
        }
    );
    assert_eq!(
        batch(&["cloud", "wait", "3", "--timeout", "600"]),
        BatchCommand::Wait {
            batch: 3,
            timeout: 600
        }
    );
    assert_eq!(
        parse(&["cloud", "wait", "3", "--timeout", "601"]).unwrap_err(),
        CliError::InvalidValue("--timeout")
    );
    assert_eq!(
        parse(&["cloud", "wait", "three"]).unwrap_err(),
        CliError::InvalidValue("BATCH")
    );
    assert_eq!(batch(&["cloud", "batches"]), BatchCommand::List);
    assert!(parse(&["cloud", "batches", "extra"]).is_err());
}

#[test]
fn cloud_create_asks_for_machines_of_one_provider() {
    use crate::control::batch::{CreateOp, Shape};
    assert_eq!(
        batch(&[
            "cloud",
            "create",
            "coder",
            "--count",
            "10",
            "--template",
            "dev",
            "--preset",
            "large"
        ]),
        BatchCommand::Create(CreateOp {
            provider: "coder".into(),
            count: 10,
            template: Some("dev".into()),
            preset: Some("large".into()),
            shape: Shape::Machine,
            install: true,
            setup: true,
        })
    );
    assert_eq!(
        batch(&[
            "cloud",
            "create",
            "azure",
            "--sandbox",
            "--no-install",
            "--no-setup"
        ]),
        BatchCommand::Create(CreateOp {
            provider: "azure".into(),
            count: 1,
            template: None,
            preset: None,
            shape: Shape::Sandbox,
            install: false,
            setup: false,
        })
    );
    for count in ["0", "17"] {
        assert_eq!(
            parse(&["cloud", "create", "coder", "--count", count]).unwrap_err(),
            CliError::InvalidValue("--count")
        );
    }
    assert_eq!(
        parse(&["cloud", "create"]).unwrap_err(),
        CliError::MissingArgument {
            command: "cloud",
            what: "PROVIDER"
        }
    );
}
