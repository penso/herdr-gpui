use super::*;

fn paste(text: &str, bracketed_paste: bool) -> String {
    let modes = Modes {
        bracketed_paste,
        ..Modes::default()
    };
    bytes(&ClientPaneInputEvent::Paste(text.to_owned()), &modes)
}

#[test]
fn bracketed_paste_wraps_and_cannot_be_ended_early() {
    assert_eq!(paste("ls\n", true), "\x1b[200~ls\n\x1b[201~");
    assert_eq!(
        paste("a\x1b[201~rm -rf ~\n", true),
        "\x1b[200~a[201~rm -rf ~\n\x1b[201~"
    );
}

#[test]
fn plain_paste_sends_returns_for_newlines() {
    assert_eq!(paste("one\ntwo\r\nthree", false), "one\rtwo\rthree");
}
