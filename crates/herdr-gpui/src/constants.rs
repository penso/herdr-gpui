//! Values fixed at build time: the product name the OS sees and the release
//! identity the updater and any credential store key off.

// Every window carries the product name; the focused space follows it.
pub(crate) const WINDOW_TITLE: &str = "Herdr";

// Release builds embed the same calendar version (YYYYMMDD.COUNTER) used for the
// tag, the bundle, and the downloadable artifacts. Local builds are not releases,
// so they carry no version the updater or an issue report could act on.
pub(crate) const APP_VERSION: &str = match option_env!("HERDR_RELEASE_VERSION") {
    Some(version) => version,
    None => "dev",
};

// Only the signed release pipeline sets that tag. Local `just run` and worktree
// builds are unsigned, so features that depend on a stable code identity (such as
// macOS Keychain access) must treat them as development builds.
pub(crate) const RELEASE_BUILD: bool = option_env!("HERDR_RELEASE_VERSION").is_some();
