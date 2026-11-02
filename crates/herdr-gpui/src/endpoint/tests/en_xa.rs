use super::*;
use crate::i18n;
use herdr_i18n::{Locale, Pseudo};

#[test]
fn a_device_state_is_a_message_under_en_xa_and_english_for_commands() {
    let mut endpoint = Endpoint::new(LOCAL.into(), "Local".into(), ConnectTarget::Local, true);
    i18n::show(&Pseudo::Accented.locale());
    let mut shown = vec![endpoint.status_label()];
    endpoint.version_mismatch = Some(VersionMismatch::DaemonOutdated {
        server_version: None,
    });
    shown.push(endpoint.status_label());
    let command = endpoint.status();
    endpoint.enabled = false;
    shown.push(endpoint.status_label());
    i18n::show(&Locale::FALLBACK);
    for line in shown {
        assert!(herdr_i18n::pseudo::is_marked(&line), "{line}");
    }
    assert_eq!(command, "Herdr update needed");
}
