use super::*;

#[test]
fn the_launcher_defaults_to_plus_and_is_read_and_saved_by_name() {
    assert_eq!(
        crate::config::Config::parse("").unwrap().chat.launcher,
        ChatLauncher::Plus
    );
    for &launcher in ChatLauncher::ALL {
        let text = format!("[chat]\nlauncher = \"{}\"\n", launcher.name());
        let config = crate::config::Config::parse(&text).unwrap();
        assert_eq!(config.chat.launcher, launcher);
    }
    assert!(crate::config::Config::parse("[chat]\nlauncher = \"dock\"\n").is_err());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    std::fs::write(&path, "[chat]\nlayout = \"cards\"\n").unwrap();
    for &launcher in ChatLauncher::ALL {
        crate::config::Config::save_preference_path(
            crate::config::preferences::Preference::ChatLauncher(launcher),
            &path,
        )
        .unwrap();
        let config =
            crate::config::Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config.chat.launcher, launcher);
        assert_eq!(config.chat.layout, ChatLayoutMode::Cards);
    }
}

#[test]
fn every_launcher_has_its_own_name_and_label() {
    let names: std::collections::HashSet<_> = ChatLauncher::ALL
        .iter()
        .map(|launcher| launcher.name())
        .collect();
    let labels: std::collections::HashSet<_> = ChatLauncher::ALL
        .iter()
        .map(|launcher| launcher.label())
        .collect();
    assert_eq!(names.len(), ChatLauncher::ALL.len());
    assert_eq!(labels.len(), ChatLauncher::ALL.len());
}
