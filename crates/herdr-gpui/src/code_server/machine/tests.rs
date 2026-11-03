#![allow(clippy::unwrap_used)]
use super::*;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herdr-vscode-machine-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_server_without_machine_settings_follows_the_pages_color_scheme() {
    let home = folder("missing");
    let path = path(&home);
    assert!(path.ends_with(".vscode-server/data/Machine/settings.json"));
    assert!(seed(&path).unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), SETTINGS);
    let parsed: serde_json::Value = serde_json::from_str(SETTINGS).unwrap();
    assert_eq!(parsed["window.autoDetectColorScheme"], true);
    // Written once: the file is the user's from then on.
    assert!(!seed(&path).unwrap());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_empty_file_is_filled_but_the_users_settings_are_kept() {
    let home = folder("kept");
    let path = path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "").unwrap();
    assert!(seed(&path).unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), SETTINGS);
    let users = "{ \"workbench.colorTheme\": \"Solarized Dark\" }";
    fs::write(&path, users).unwrap();
    assert!(!seed(&path).unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), users);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_host_script_writes_the_same_settings() {
    // serve.sh writes them with printf, whose \n and \t become these.
    let script = include_str!("../host/serve.sh");
    let printed = SETTINGS.replace('\n', "\\n").replace('\t', "\\t");
    assert!(script.contains(&format!("printf '{printed}'")), "{printed}");
    assert!(script.contains(r#"machine="$HOME/.vscode-server/data/Machine""#));
    assert!(script.contains(r#"if [ ! -s "$machine/settings.json" ]; then"#));
}
