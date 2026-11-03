use super::*;
use std::collections::HashMap;

fn from(vars: &[(&str, &str)]) -> Paths {
    let vars = vars
        .iter()
        .map(|(name, value)| ((*name).to_owned(), PathBuf::from(value)))
        .collect::<HashMap<_, _>>();
    Paths::from_vars(|name| vars.get(name).cloned()).unwrap()
}

#[test]
fn the_service_directories_win_over_everything_else() {
    let paths = from(&[
        ("HOME", "/home/me"),
        ("HERDR_AUTOMATE_HOME", "/elsewhere"),
        (STATE_DIR, "/state/herdr-automate"),
        (CONFIG_DIR, "/config/herdr-automate"),
    ]);
    assert_eq!(paths.state, PathBuf::from("/state/herdr-automate"));
    assert_eq!(paths.config, PathBuf::from("/config/herdr-automate"));
    // Both are needed; one alone is ignored.
    let paths = from(&[("HERDR_AUTOMATE_HOME", "/elsewhere"), (STATE_DIR, "/state")]);
    assert_eq!(paths, Paths::under(Path::new("/elsewhere")));
}
