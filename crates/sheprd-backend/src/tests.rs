use super::*;

#[test]
fn a_size_needs_a_column_and_a_row() {
    assert_eq!(Size::new(0, 24), Err(Error::EmptySize));
    assert_eq!(Size::new(80, 0), Err(Error::EmptySize));
    assert_eq!(
        Size::new(MAX_SIDE + 1, 1),
        Err(Error::LargeSize {
            cols: MAX_SIDE + 1,
            rows: 1
        })
    );
    assert_eq!(
        Size::new(MAX_SIDE, MAX_SIDE),
        Err(Error::LargeSize {
            cols: MAX_SIDE,
            rows: MAX_SIDE
        })
    );
    let size = Size::new(80, 24).unwrap();
    assert_eq!((size.cols(), size.rows()), (80, 24));
}

#[test]
fn a_terminal_id_is_never_empty() {
    assert_eq!(TerminalId::new(""), Err(Error::EmptyTerminalId));
    let id = TerminalId::new("box/t7").unwrap();
    assert_eq!(id.as_str(), "box/t7");
    assert_eq!(id.to_string(), "box/t7");
}

#[test]
fn a_saved_terminal_id_is_checked_on_load() {
    let id: TerminalId = serde_json::from_str("\"box/t7\"").unwrap();
    assert_eq!(serde_json::to_string(&id).unwrap(), "\"box/t7\"");
    assert!(serde_json::from_str::<TerminalId>("\"t\\u0007\"").is_err());
    assert_eq!(
        TerminalId::new("x".repeat(MAX_TERMINAL_ID + 1)),
        Err(Error::InvalidTerminalId)
    );
}
