use super::*;
use std::io;

fn english(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

#[test]
fn a_failed_restore_keeps_the_cause_its_git_commands_or_worker_gave() {
    crate::i18n::show(&sheprd_i18n::Locale::FALLBACK);
    let script = sheprd_vcs::Error::Checkpoint(checkpoint::Error::Script(
        sheprd_client::Error::Disconnected,
    ));
    assert_eq!(
        english(not_restored(&script)),
        format!(
            "Checkpoint not restored: {script}: {}",
            sheprd_client::Error::Disconnected
        )
    );
    let thread = sheprd_vcs::Error::Checkpoint(checkpoint::Error::Thread(io::Error::other(
        "no threads left",
    )));
    assert_eq!(
        english(not_restored(&thread)),
        format!("Checkpoint not restored: {thread}: no threads left")
    );
    assert_eq!(
        english(not_restored(&sheprd_vcs::Error::Checkpoint(
            checkpoint::Error::Busy
        ))),
        format!("Checkpoint not restored: {}", checkpoint::Error::Busy)
    );
}
