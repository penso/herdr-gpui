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
