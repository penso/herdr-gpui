use std::{fs, path::Path};

use super::*;

/// Docks find the icon by matching a window's app ID to a desktop entry's file
/// name or `StartupWMClass`, so a drift between them loses the icon silently.
#[test]
fn the_linux_desktop_entry_is_named_for_the_app_id() -> anyhow::Result<()> {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/release")
        .join(format!("{LINUX_APP_ID}.desktop"));
    let entry = fs::read_to_string(entry)?;
    let class = format!("StartupWMClass={LINUX_APP_ID}");
    assert!(entry.lines().any(|line| line == class), "{entry}");
    Ok(())
}
