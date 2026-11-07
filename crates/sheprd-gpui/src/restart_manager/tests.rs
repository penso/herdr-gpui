use super::*;

#[test]
fn only_the_lone_restart_argument_is_handed_off() {
    for args in [
        vec![],
        vec!["--socket".into()],
        vec![RESTARTED.into(), "x".into()],
    ] {
        let mut kept: Vec<OsString> = args.clone();
        assert!(hand_off(&mut kept).is_none());
        assert_eq!(kept, args);
    }
}

#[test]
fn a_restart_without_a_launcher_opens_the_app_itself() {
    // The test executable has no herdr-gpui-launcher.exe beside it.
    let mut args = vec![OsString::from(RESTARTED)];
    assert!(hand_off(&mut args).is_none());
    assert!(args.is_empty());
}
