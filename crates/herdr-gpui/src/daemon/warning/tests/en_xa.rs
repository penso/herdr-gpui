use super::*;
use crate::i18n;
use herdr_i18n::{Locale, Pseudo};

#[test]
fn every_warning_is_a_message_under_en_xa() {
    i18n::show(&Pseudo::Accented.locale());
    let shown: Vec<_> = [
        UntrustedEndpoint::PeerUser,
        UntrustedEndpoint::NotSocket,
        UntrustedEndpoint::OtherSocket,
        UntrustedEndpoint::SocketPermissions,
        UntrustedEndpoint::DirectoryPermissions,
    ]
    .into_iter()
    .flat_map(|reason| {
        let notice = LocalPeerWarning::new(reason, "/tmp/herdr/herdr-client.sock".into())
            .notice(std::time::Instant::now());
        [notice.title, notice.body.unwrap_or_default()]
    })
    .collect();
    i18n::show(&Locale::FALLBACK);
    for text in shown {
        let first = text.lines().next().unwrap_or_default();
        assert!(herdr_i18n::pseudo::is_marked(first), "{text}");
    }
}

/// en-XB draws the warning right to left through Fluent's direction marks,
/// which the notice keeps.
#[test]
fn the_warning_keeps_its_direction_marks_under_en_xb() {
    i18n::show(&Pseudo::Bidi.locale());
    let notice = LocalPeerWarning::new(
        UntrustedEndpoint::SocketPermissions,
        "/tmp/herdr/herdr-client.sock".into(),
    )
    .notice(std::time::Instant::now());
    i18n::show(&Locale::FALLBACK);
    let body = notice.body.unwrap_or_default();
    assert!(notice.title.contains('\u{202e}'), "{}", notice.title);
    assert!(body.contains('\u{202e}'), "{body}");
    assert!(
        body.ends_with("chmod -- go-w '/tmp/herdr/herdr-client.sock'"),
        "{body}"
    );
}
