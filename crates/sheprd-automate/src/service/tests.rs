use super::*;

#[test]
fn the_service_keeps_the_directories_it_was_installed_with() {
    let paths = Paths {
        state: PathBuf::from("/custom/state 100%/herdr-automate"),
        config: PathBuf::from("/custom/config/herdr-automate"),
    };
    let Ok(service) = describe(Path::new("/opt/herdr-automate"), &paths) else {
        // No home directory to put the service in.
        return;
    };
    let content = service.content.unwrap();
    assert!(content.contains(STATE_DIR), "{content}");
    assert!(content.contains(CONFIG_DIR), "{content}");
    assert!(
        content.contains("/custom/config/herdr-automate"),
        "{content}"
    );
    if cfg!(target_os = "linux") {
        assert!(
            content.contains(
                "Environment=\"HERDR_AUTOMATE_STATE_DIR=/custom/state 100%%/herdr-automate\""
            ),
            "{content}"
        );
    }
}
