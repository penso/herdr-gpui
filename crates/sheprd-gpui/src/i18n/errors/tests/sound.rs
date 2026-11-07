use super::*;
use crate::sound::Error;

#[test]
fn every_sound_variant_has_a_message() {
    assert_localized(&[
        Error::Cancelled,
        Error::ConfigSize,
        Error::Decode(rodio::decoder::DecoderError::UnrecognizedFormat),
        Error::Device(rodio::DeviceSinkError::NoDevice),
        Error::File {
            path: "/tmp/ding.mp3".into(),
            source: std::io::Error::other("disk on fire"),
        },
        Error::FileSize,
        Error::Stream(rodio::cpal::StreamError::DeviceNotAvailable),
        Error::Timeout,
    ]);
}
