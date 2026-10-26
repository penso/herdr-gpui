use super::*;

#[core::prelude::v1::test]
fn the_truncation_note_names_the_limits_that_cut_the_block() {
    assert_eq!(
        truncation_note(),
        "Preview limited to 512 lines and 4,000 characters per line; Copy code includes the full block."
    );
    for (number, text) in [
        (0, "0"),
        (999, "999"),
        (1_000, "1,000"),
        (1_234_567, "1,234,567"),
    ] {
        assert_eq!(grouped(number), text);
    }
}
