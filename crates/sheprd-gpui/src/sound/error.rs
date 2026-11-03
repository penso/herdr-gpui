//! Why a notification sound did not play or its settings did not load.

use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
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
}

// The audio library's failures convert straight into the crate's error, so
// playback keeps using `?` on them.
impl From<rodio::decoder::DecoderError> for crate::Error {
    fn from(error: rodio::decoder::DecoderError) -> Self {
        Self::Sound(error.into())
    }
}

impl From<rodio::DeviceSinkError> for crate::Error {
    fn from(error: rodio::DeviceSinkError) -> Self {
        Self::Sound(error.into())
    }
}

impl From<rodio::cpal::StreamError> for crate::Error {
    fn from(error: rodio::cpal::StreamError) -> Self {
        Self::Sound(error.into())
    }
}
