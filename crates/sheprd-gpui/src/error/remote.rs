//! Errors from `herdr-remote`, wrapped once as `Error::Remote`. A cloud,
//! provider, sandbox image, or teleport failure raised here reaches the root
//! through `sheprd_remote::Error` too, so each has one place to be matched.

use super::Error;

/// Not `#[from]`: `herdr-remote` loads the app's config through a loader the
/// app installs, and boxes what it returns. That boxed failure is the app's
/// own error coming back, so it is unboxed rather than wrapped; any other
/// boxed failure, and every other variant, stays a `herdr-remote` error.
impl From<sheprd_remote::Error> for Error {
    fn from(error: sheprd_remote::Error) -> Self {
        match error {
            sheprd_remote::Error::Config(error) => match error.downcast::<Error>() {
                Ok(error) => *error,
                Err(error) => Self::Remote(sheprd_remote::Error::Config(error)),
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
                sheprd_remote::Error::from(error).into()
            }
        })*
    };
}

remote_errors! {
    // keep-sorted start
    #[cfg(feature = "azure")] sheprd_remote::azure::Error,
    #[cfg(feature = "boat")] sheprd_remote::boat::Error,
    #[cfg(feature = "cloud")] sheprd_remote::cloud::Error,
    #[cfg(feature = "cloud")] sheprd_remote::cloud::ProviderError,
    #[cfg(feature = "cloud")] sheprd_remote::cloud::image::Error,
    #[cfg(feature = "coder")] sheprd_remote::coder::Error,
    #[cfg(feature = "container")] sheprd_remote::container::Error,
    #[cfg(feature = "daytona")] sheprd_remote::daytona::Error,
    #[cfg(feature = "opensandbox")] sheprd_remote::opensandbox::Error,
    #[cfg(feature = "railway")] sheprd_remote::railway::Error,
    #[cfg(feature = "sprites")] sheprd_remote::sprites::Error,
    sheprd_remote::teleport::Error,
    // keep-sorted end
}
