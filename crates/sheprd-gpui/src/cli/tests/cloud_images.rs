use super::*;
use control::{CloudCommand, ControlCommand, Invocation};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

#[test]
fn cloud_images_lists_or_builds() {
    for args in [&["cloud", "images"][..], &["cloud", "images", "list"]] {
        assert_eq!(
            parse(args).unwrap().command,
            ControlCommand::Cloud(CloudCommand::Images)
        );
    }
    assert_eq!(
        parse(&["cloud", "images", "build"]).unwrap().command,
        ControlCommand::Cloud(CloudCommand::ImagesBuild(None))
    );
    assert_eq!(
        parse(&["cloud", "images", "build", "daytona"])
            .unwrap()
            .command,
        ControlCommand::Cloud(CloudCommand::ImagesBuild(Some("daytona".into())))
    );
    assert!(
        parse(&["cloud", "images", "reset"]).is_err(),
        "reset stays in the app"
    );
}
