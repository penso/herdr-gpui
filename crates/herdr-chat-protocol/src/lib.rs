//! The attach wire between the `herdr-acp` chat program and its clients.
#![doc = include_str!("../README.md")]

mod agents;
pub mod assist;
mod change;
pub mod display;
mod endpoint;
mod error;
mod frame;
mod media;
mod model;
mod wire;

pub use agents::{Agent, UnknownAgent};
pub use change::Change;
pub use endpoint::{
    ENDPOINT_ENV, EndpointId, TOKEN, endpoint_address, endpoint_dir, endpoint_dir_with,
    endpoint_path,
};
pub use error::{Error, Result};
pub use frame::{MAX_FRAME_SIZE, encode_frame, read_frame, write_frame};
pub use media::{Author, Image, ImageType, MAX_IMAGE_DATA};
pub use model::*;
pub use wire::*;

#[cfg(test)]
mod tests;
