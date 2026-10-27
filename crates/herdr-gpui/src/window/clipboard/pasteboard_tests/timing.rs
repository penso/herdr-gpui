//! Characterize #761 without timers or the user's general pasteboard. This is
//! the acquisition boundary, not proof of native Cmd-V dispatch timing.
use super::*;

#[test]
fn restoration_before_acquisition_is_not_a_mid_read_revision_change() {
    objc2::rc::autoreleasepool(|_| {
        for restored in [Some("previous clipboard"), None] {
            let transcript = "dictated text\n语音输入";
            let board =
                pasteboard(&[&[("public.utf8-plain-text", transcript.as_bytes().to_vec())]]);
            let at_keystroke = board.changeCount();
            // A materialized item owns its bytes; it is a control for what a
            // future keystroke-time capture must preserve through preparation.
            let captured = read_pasteboard(&board, false).unwrap().unwrap();
            // Model executor/helper startup or held-input delay explicitly:
            // restoration finishes before the production reader is called.
            board.clearContents();
            if let Some(text) = restored {
                assert!(board.setString_forType(
                    &NSString::from_str(text),
                    &NSString::from_str("public.utf8-plain-text"),
                ));
            }
            assert_ne!(board.changeCount(), at_keystroke);
            let after_restoration = board.changeCount();

            let acquired_late = read_pasteboard(&board, false).unwrap();

            // The current revision fence correctly accepts a stable read, but
            // has no earlier revision with which to detect the lost transcript.
            assert_eq!(board.changeCount(), after_restoration);
            assert_eq!(
                acquired_late.and_then(|item| item.text()).as_deref(),
                restored
            );
            assert_eq!(captured.text().as_deref(), Some(transcript));
            board.clearContents();
        }
    });
}
