use super::*;
use crate::window::clipboard::Error;

/// `Native` wraps the macOS reader's error, whose module is private to the
/// window, so it cannot be built here; it shows its English `Display`.
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
