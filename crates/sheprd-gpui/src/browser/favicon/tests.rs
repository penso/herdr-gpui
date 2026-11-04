#![allow(clippy::unwrap_used)]
use super::*;

fn report(pixels: &[u8]) -> String {
    serde_json::json!({ "kind": "favicon", "pixels": pixels }).to_string()
}

#[test]
fn reported_pixels_become_a_bgra_icon() {
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    pixels[..4].copy_from_slice(&[10, 20, 30, 255]);
    let Some(Favicon::Icon(icon)) = Favicon::parse(&report(&pixels)) else {
        panic!("expected an icon");
    };
    let size = icon.size(0);
    assert_eq!((size.width.0, size.height.0), (SIZE as i32, SIZE as i32));
    // The red and blue channels trade places.
    assert_eq!(&icon.as_bytes(0).unwrap()[..4], &[30, 20, 10, 255]);
}

#[test]
fn a_page_without_an_icon_reports_it_missing() {
    for body in [
        r#"{"kind":"favicon","pixels":null}"#,
        r#"{"kind":"favicon"}"#,
    ] {
        assert!(
            matches!(Favicon::parse(body), Some(Favicon::Missing)),
            "{body}"
        );
    }
}

#[test]
fn malformed_reports_and_other_messages_are_ignored() {
    let whole = (SIZE * SIZE * 4) as usize;
    for body in [
        report(&vec![0; whole - 1]),
        report(&vec![0; whole + 4]),
        report(&[]),
        r#"{"kind":"favicon","pixels":[256]}"#.to_owned(),
        r#"{"kind":"favicon","pixels":"AAAA"}"#.to_owned(),
        r#"{"kind":"cancel"}"#.to_owned(),
        "not json".to_owned(),
    ] {
        assert!(Favicon::parse(&body).is_none(), "{body}");
    }
}

#[test]
fn the_script_is_called_with_its_size_and_pause() {
    let script = script();
    assert!(script.starts_with("// Sheprd's favicon reader."));
    assert!(script.ends_with(&format!("}})({SIZE}, {PAUSE_MS});")));
}
