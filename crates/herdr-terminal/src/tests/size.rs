use super::*;

#[test]
fn resizing_reflows_and_keeps_bounds() {
    let mut emulator = fed(6, 3, b"abcdef");
    emulator.resize(size(3, 3));
    assert_eq!(emulator.size(), size(3, 3));
    let frame = emulator.frame();
    frame.validate().unwrap();
    assert_eq!(frame.cells.len(), 9);
    // The cursor's row stays put, so the wrapped start moves into history.
    assert_eq!(line(&frame, 0), "def");
    emulator.scroll(Scroll::Lines(1));
    assert_eq!(line(&emulator.frame(), 0), "abc");
    assert_eq!(line(&emulator.frame(), 1), "def");
}

#[test]
fn size_errors_wrap_the_backends() {
    let error = Error::from(Size::new(0, 1).unwrap_err());
    assert_eq!(error, Error::Backend(herdr_backend::Error::EmptySize));
    assert_eq!(
        error.to_string(),
        herdr_backend::Error::EmptySize.to_string()
    );
}
