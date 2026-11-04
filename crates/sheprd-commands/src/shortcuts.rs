use super::CommandInfo;

mod linux;
mod macos;
mod windows;

/// Linux and Windows leave the platform key (Super, Win) to the desktop, so
/// their shortcuts and text-field edit keys take Ctrl where macOS takes Cmd.
pub const CTRL_SHORTCUTS: bool = cfg!(any(target_os = "linux", windows));

impl CommandInfo {
    pub fn default_shortcuts(&self) -> &'static [&'static str] {
        if cfg!(target_os = "linux") {
            linux::defaults(self.command)
        } else if cfg!(target_os = "windows") {
            windows::defaults(self.command)
        } else {
            macos::defaults(self.command)
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn base_shortcuts(&self) -> &'static [&'static str] {
        macos::defaults(self.command)
    }
}

/// Existing cross-platform tests name the base catalog key; resolve just
/// those defaults, never keys explicitly supplied by a test's config.
#[cfg(any(test, feature = "test-support"))]
pub fn platform_default(base: &str) -> &str {
    super::COMMANDS
        .iter()
        .find_map(|info| {
            let index = info.base_shortcuts().iter().position(|key| *key == base)?;
            info.default_shortcuts().get(index).copied()
        })
        .unwrap_or(base)
}

#[cfg(test)]
mod tests;
