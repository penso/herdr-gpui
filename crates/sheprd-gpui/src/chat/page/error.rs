/// Why a chat's page could not be shown.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("no folder for pages")]
    Folder,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Only builds that show no pages open a file in the system browser.
    #[cfg_attr(
        all(any(target_os = "macos", windows), not(test)),
        expect(dead_code, reason = "pages show in the window here")
    )]
    #[error("not a file path")]
    NotFile,
    #[error("the page is too large")]
    TooLarge,
    // keep-sorted end
}
