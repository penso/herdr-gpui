use super::*;
use crate::window::clipboard::Error;

#[test]
fn every_clipboard_variant_has_a_message() {
    assert_localized(&[
        Error::Changed,
        Error::Encoding(String::from_utf8(vec![0xff]).unwrap_err().utf8_error()),
        Error::Process(std::io::Error::other("disk on fire")),
        Error::Size { limit: 16_777_216 },
        Error::Timeout,
        Error::Unsupported,
    ]);
}

#[cfg(target_os = "macos")]
#[test]
fn every_macos_reader_variant_has_a_message() {
    use crate::window::clipboard::macos::Error as Native;
    use std::os::unix::process::ExitStatusExt as _;
    let errors = [
        Native::Exited(std::process::ExitStatus::from_raw(256)),
        Native::Missing("/Applications/Herdr.app/Contents/MacOS/herdr-gpui".into()),
        Native::Process(std::io::Error::other("disk on fire")),
        Native::Response,
        Native::TextEncoding,
    ];
    assert_localized(&errors);
    assert_localized(&errors.map(Error::Native));
}
