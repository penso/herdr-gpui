//! The host OS for keystroke text. GPUI takes it as an argument so a browser
//! build can follow its host; a native build is the OS it was compiled for,
//! which lets input paths without an `App` read it too.

use gpui::{InvalidKeystrokeError, Keystroke, Modifiers, OperatingSystem};

pub(crate) const HOST_OS: OperatingSystem = if cfg!(target_os = "macos") {
    OperatingSystem::Mac
} else if cfg!(target_os = "windows") {
    OperatingSystem::Windows
} else if cfg!(target_os = "freebsd") {
    OperatingSystem::FreeBsd
} else {
    OperatingSystem::Linux
};

/// Parses keystroke text that isn't a literal, such as a config value.
pub(crate) fn parse_keystroke(text: &str) -> Result<Keystroke, InvalidKeystrokeError> {
    Keystroke::parse_with_os(text, HOST_OS)
}

/// Whether the platform's primary modifier is held: command on macOS,
/// control elsewhere.
pub(crate) fn secondary(modifiers: &Modifiers) -> bool {
    if matches!(HOST_OS, OperatingSystem::Mac) {
        modifiers.platform
    } else {
        modifiers.control
    }
}

/// [`Modifiers`] with only the platform's primary modifier held.
#[cfg(test)]
pub(crate) fn secondary_key() -> Modifiers {
    if matches!(HOST_OS, OperatingSystem::Mac) {
        Modifiers::command()
    } else {
        Modifiers::control()
    }
}
