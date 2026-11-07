//! How a Windows window reaches the screen. GPUI draws through a
//! DirectComposition target it makes topmost, which puts its drawing above
//! the window's child windows. A page is such a child window, so its title
//! and address arrive while GPUI's opaque paint hides all of its content.
//! Without DirectComposition, GPUI draws into the window itself and a page
//! shows above that drawing, as `browser::view::present` expects.
//!
//! Only DirectComposition blends with what lies behind the window, so a
//! window that starts translucent keeps it, and its pages stay hidden. GPUI
//! reads this once, as it starts: changing the backdrop takes a restart.
//!
//! GPUI takes the choice only from its environment variable. Writing the
//! process's environment has no safe form.
#![cfg_attr(windows, allow(unsafe_code))]

use gpui::Application;

#[cfg(windows)]
const DISABLE: &str = "GPUI_DISABLE_DIRECT_COMPOSITION";

/// Starts GPUI for windows that begin `translucent` or not. The variable
/// lasts only while GPUI starts, so terminals and other programs the app
/// starts never inherit it. A value the user set is left to GPUI.
pub(crate) fn application(translucent: bool) -> Application {
    #[cfg(windows)]
    if !translucent && std::env::var_os(DISABLE).is_none() {
        // SAFETY: Windows guards its environment block, and the standard
        // library's accessors, which other threads use, take its lock.
        unsafe { std::env::set_var(DISABLE, "1") };
        let application = gpui_platform::application();
        // SAFETY: as above.
        unsafe { std::env::remove_var(DISABLE) };
        return application;
    }
    let _ = translucent;
    gpui_platform::application()
}
