use super::*;
use crate::backdrop::Error;
use std::{io, path::PathBuf};

#[test]
fn every_backdrop_variant_has_a_message() {
    let path = || PathBuf::from("/tmp/sky.png");
    assert_localized(&[
        Error::Decode {
            path: path(),
            source: image::ImageError::IoError(io::Error::other("truncated")),
        },
        Error::EmptyImage,
        Error::ImagePathEncoding,
        Error::NotAFile { path: path() },
        Error::Opacity,
        Error::Read {
            path: path(),
            source: io::Error::other("disk on fire"),
        },
        Error::RelativeImage,
        Error::TooLarge { path: path() },
    ]);
}
