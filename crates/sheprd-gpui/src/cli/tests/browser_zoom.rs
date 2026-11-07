use super::*;
use control::ZoomChange;

#[test]
fn browser_zoom_takes_a_step_a_reset_or_a_percent() {
    let mode = |args: &[&str]| LaunchOptions::parse(args.iter().copied()).map(|o| o.mode);
    let zoom = |change| browser(BrowserCommand::Zoom { id: 3, change });
    for (change, expected) in [
        ("+", ZoomChange::In),
        ("in", ZoomChange::In),
        ("-", ZoomChange::Out),
        ("out", ZoomChange::Out),
        ("reset", ZoomChange::Reset),
        ("150", ZoomChange::Percent { percent: 150 }),
        ("75%", ZoomChange::Percent { percent: 75 }),
    ] {
        assert_eq!(
            mode(&["browser", "zoom", "3", change]).unwrap(),
            zoom(expected),
            "{change}"
        );
    }
    for (args, expected) in [
        (
            &["browser", "zoom", "3"][..],
            CliError::MissingArgument {
                command: "browser zoom",
                what: "+|-|reset|PERCENT",
            },
        ),
        (
            &["browser", "zoom", "x", "+"],
            CliError::InvalidValue("TAB (an ID from `browser list`)"),
        ),
        (
            &["browser", "zoom", "3", "49"],
            CliError::InvalidValue("PERCENT (50-300)"),
        ),
        (
            &["browser", "zoom", "3", "301%"],
            CliError::InvalidValue("PERCENT (50-300)"),
        ),
        (
            &["browser", "zoom", "3", "big"],
            CliError::InvalidValue("PERCENT (50-300)"),
        ),
        (
            &["browser", "zoom", "3", "+", "more"],
            CliError::UnexpectedArgument("more".into()),
        ),
    ] {
        assert_eq!(mode(args).unwrap_err(), expected, "{args:?}");
    }
}
