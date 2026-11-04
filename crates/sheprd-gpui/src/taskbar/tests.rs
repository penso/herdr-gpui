use std::path::MAIN_SEPARATOR;

use super::*;

#[cfg(windows)]
const ROOT: &str = r"C:\";
#[cfg(not(windows))]
const ROOT: &str = "/";

fn gui_in(directory: &str) -> PathBuf {
    Path::new(directory).join("herdr-gpui.exe")
}

#[test]
fn relaunch_points_a_pin_at_the_launcher_beside_the_gui() {
    let directory = format!("{ROOT}Herdr");
    let launcher = format!("{directory}{MAIN_SEPARATOR}herdr-gpui-launcher.exe");

    let relaunch = relaunch(&gui_in(&directory));

    assert_eq!(
        relaunch,
        Some(Relaunch {
            command: OsString::from(format!("\"{launcher}\"")),
            display_name: "Sheprd",
            icon: OsString::from(format!("{launcher},-1")),
            id: crate::constants::APP_ID,
            launcher: PathBuf::from(launcher),
        })
    );
}

#[test]
fn relaunch_quotes_a_launcher_path_with_spaces() {
    let directory = format!("{ROOT}Program Files{MAIN_SEPARATOR}Sheprd");
    let launcher = format!("{directory}{MAIN_SEPARATOR}herdr-gpui-launcher.exe");

    let relaunch = relaunch(&gui_in(&directory)).map(|relaunch| (relaunch.command, relaunch.icon));

    assert_eq!(
        relaunch,
        Some((
            OsString::from(format!("\"{launcher}\"")),
            OsString::from(format!("{launcher},-1"))
        ))
    );
}

#[test]
fn relaunch_needs_a_parent_directory() {
    assert_eq!(relaunch(Path::new(ROOT)), None);
    assert_eq!(relaunch(Path::new("")), None);
}
