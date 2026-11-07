use super::*;
use crate::link::{Error, Link};

#[test]
fn a_lone_link_is_handed_to_the_app() {
    let options = LaunchOptions::parse(["herdr-gpui://device?id=local"]).unwrap();
    assert_eq!(options.target, ConnectTarget::Local);
    assert_eq!(
        options.mode,
        LaunchMode::Link(Link::Device { id: "local".into() })
    );
}

#[test]
fn a_link_takes_no_other_argument() {
    for args in [
        &["herdr-gpui://device?id=local", "--socket"][..],
        &[
            "herdr-gpui://device?id=local",
            "herdr-gpui://device?id=local",
        ],
    ] {
        assert_eq!(
            LaunchOptions::parse(args).unwrap_err(),
            CliError::LinkArguments
        );
    }
    // Options before it are not a link invocation at all.
    assert_eq!(
        LaunchOptions::parse(["--dev", "herdr-gpui://device?id=local"]).unwrap_err(),
        CliError::UnknownOption("herdr-gpui://device?id=local".into())
    );
}

#[test]
fn a_bad_link_is_reported_as_a_link() {
    assert_eq!(
        LaunchOptions::parse(["herdr-gpui://run?command=calc"]).unwrap_err(),
        CliError::Link(Error::UnknownAction("run".into()))
    );
    assert_eq!(
        LaunchOptions::parse(["HERDR-GPUI:nothing"]).unwrap_err(),
        CliError::Link(Error::UnexpectedPart)
    );
}
