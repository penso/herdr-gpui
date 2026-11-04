use super::*;

#[test]
fn markers_are_counted_and_stripped_with_their_padding() {
    assert_eq!(depth("plain"), 0);
    assert_eq!(depth("> > ```"), 2);
    assert_eq!(depth(">>x"), 2);
    assert_eq!(strip("> >   ```", 2), Some("  ```"));
    assert_eq!(strip(">x", 1), Some("x"));
    assert_eq!(strip("> one", 2), None);
    assert_eq!(strip("as is", 0), Some("as is"));
}
