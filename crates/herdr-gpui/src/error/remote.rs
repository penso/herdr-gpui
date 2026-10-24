//! Errors from `herdr-remote`, wrapped once as `Error::Remote`. A cloud,
//! provider, sandbox image, or teleport failure raised here reaches the root
//! through `herdr_remote::Error` too, so each has one place to be matched.

use super::Error;

/// Not `#[from]`: `herdr-remote` loads the app's config through a loader the
/// app installs, and boxes what it returns. That boxed failure is the app's
/// own error coming back, so it is unboxed rather than wrapped; any other
/// boxed failure, and every other variant, stays a `herdr-remote` error.
impl From<herdr_remote::Error> for Error {
    fn from(error: herdr_remote::Error) -> Self {
        match error {
            herdr_remote::Error::Config(error) => match error.downcast::<Error>() {
                Ok(error) => *error,
                Err(error) => Self::Remote(herdr_remote::Error::Config(error)),
            },
            error => Self::Remote(error),
        }
    }
}

/// A part of `herdr-remote`'s own error, wrapped as that crate wraps it.
macro_rules! remote_errors {
    ($($(#[$meta:meta])* $error:path),* $(,)?) => {
        $($(#[$meta])*
        impl From<$error> for Error {
            fn from(error: $error) -> Self {
                herdr_remote::Error::from(error).into()
            }
        })*
    };
}

remote_errors! {
    // keep-sorted start
    #[cfg(feature = "azure")] herdr_remote::azure::Error,
    #[cfg(feature = "boat")] herdr_remote::boat::Error,
    #[cfg(feature = "cloud")] herdr_remote::cloud::Error,
    #[cfg(feature = "cloud")] herdr_remote::cloud::ProviderError,
    #[cfg(feature = "cloud")] herdr_remote::cloud::image::Error,
    #[cfg(feature = "coder")] herdr_remote::coder::Error,
    #[cfg(feature = "container")] herdr_remote::container::Error,
    #[cfg(feature = "daytona")] herdr_remote::daytona::Error,
    #[cfg(feature = "opensandbox")] herdr_remote::opensandbox::Error,
    #[cfg(feature = "railway")] herdr_remote::railway::Error,
    #[cfg(feature = "sprites")] herdr_remote::sprites::Error,
    herdr_remote::teleport::Error,
    // keep-sorted end
}
