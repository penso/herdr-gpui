#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn pump_copies_everything_and_stops_at_the_end() {
    let input = vec![7u8; CHUNK * 2 + 5];
    let mut output = Vec::new();
    pump(input.as_slice(), &mut output).unwrap();
    assert_eq!(output, input);
}
