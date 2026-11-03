use super::*;
use std::io;

fn english(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

#[test]
fn a_failed_restore_keeps_the_cause_its_git_commands_or_worker_gave() {
    crate::i18n::show(&herdr_i18n::Locale::FALLBACK);
    let script =
        herdr_vcs::Error::Checkpoint(checkpoint::Error::Script(herdr_client::Error::Disconnected));
    assert_eq!(
        english(not_restored(&script)),
        format!(
            "Checkpoint not restored: {script}: {}",
            herdr_client::Error::Disconnected
        )
    );
    let thread = herdr_vcs::Error::Checkpoint(checkpoint::Error::Thread(io::Error::other(
        "no threads left",
    )));
    assert_eq!(
        english(not_restored(&thread)),
        format!("Checkpoint not restored: {thread}: no threads left")
    );
    assert_eq!(
        english(not_restored(&herdr_vcs::Error::Checkpoint(
            checkpoint::Error::Busy
        ))),
        format!("Checkpoint not restored: {}", checkpoint::Error::Busy)
    );
}
