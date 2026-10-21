//! Sound configuration and playback failures.

use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Audio playback cancelled")]
    Cancelled,
    #[error("Sound configuration exceeds 1 MiB")]
    ConfigSize,
    #[error("Could not decode MP3 sound: {0}")]
    Decode(#[from] rodio::decoder::DecoderError),
    #[error("Could not open audio output: {0}")]
    Device(#[from] rodio::DeviceSinkError),
    #[error("Could not read sound file {}: {source}", path.display())]
    File {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Sound must be a regular file of at most 16 MiB")]
    FileSize,
    #[error("Audio output failed: {0}")]
    Stream(#[from] rodio::cpal::StreamError),
    #[error("Audio playback timed out")]
    Timeout,
    // keep-sorted end
}
