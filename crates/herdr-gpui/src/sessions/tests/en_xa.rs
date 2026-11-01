use super::*;
use crate::i18n;
use herdr_i18n::{Locale, Pseudo};

#[test]
fn a_scan_that_stopped_says_so_in_a_message_under_en_xa() {
    i18n::show(&Pseudo::Accented.locale());
    let mut sessions = Sessions {
        scanning: true,
        ..Default::default()
    };
    let (tx, rx) = mpsc::sync_channel::<Result<Vec<LocalSession>, Error>>(1);
    sessions.pending = Some(rx);
    drop(tx);
    sessions.poll_with(false, now(), || unreachable!("a scan is outstanding"));
    i18n::show(&Locale::FALLBACK);
    let error = sessions.error.unwrap_or_default();
    assert!(herdr_i18n::pseudo::is_marked(&error), "{error}");
}
