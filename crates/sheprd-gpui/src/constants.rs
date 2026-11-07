//! Values fixed at build time or by the reference design: the product name the
//! OS sees, the tab strip's geometry, and the release identity the updater and
//! any credential store key off.

// Every window carries the product name; the focused space follows it.
pub(crate) const WINDOW_TITLE: &str = "Sheprd";

// The bundle identifier and release version are the updater's, defined once
// beside the code that keys off them.
pub(crate) use sheprd_updater::{APP_ID, APP_VERSION};

// What Linux desktops know the app by: the Wayland app ID and X11 WM_CLASS that
// window rules match, and the name of the desktop entry docks find the icon
// through. Unlike `APP_ID`, no signature or installed path depends on it.
pub(crate) const LINUX_APP_ID: &str = "org.sheprd.Sheprd";

// Even tab cells, as on herdr.dev, so short labels do not collapse to a sliver.
pub(crate) const TAB_WIDTH: f32 = 64.;
// The reference strip is a shallow band: chrome, not a toolbar.
pub(crate) const TAB_HEIGHT: f32 = 24.;

// Whether this is a signed release, which the credential store keys Keychain
// access off; defined once there.
pub(crate) use sheprd_credentials::RELEASE_BUILD;

// The git commit hash embedded by build.rs for source/dev builds.
pub(crate) const BUILD_COMMIT: Option<&str> = option_env!("HERDR_BUILD_COMMIT");

#[cfg(test)]
mod tests;
