use super::*;

#[test]
fn invalid_clipboard_helper_arguments_exit_before_gui_startup() {
    for args in [
        vec!["__read-clipboard"],
        vec!["__read-clipboard", "unknown"],
        vec!["__read-clipboard", "normal", "extra"],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}
