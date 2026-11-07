//! Sandbox images: `sheprd_remote::cloud::image`, plus the builds the window
//! starts and cancels.

mod prebuild;

pub(crate) use prebuild::Prebuilds;
pub(crate) use sheprd_remote::cloud::image::*;
