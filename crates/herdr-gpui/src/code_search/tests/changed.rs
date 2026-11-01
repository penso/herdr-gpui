use super::Change;
use crate::i18n;
use herdr_i18n::{Locale, Pseudo};
use std::time::{Duration, SystemTime};

fn change(new: bool, modified: Option<SystemTime>) -> Change {
    Change {
        file: 0,
        counts: None,
        new,
        modified,
    }
}

#[test]
fn a_changed_file_says_how_long_ago_it_was_written() {
    let detail = |change: Change| {
        super::super::render::changed_detail(&change).replace(['\u{2068}', '\u{2069}'], "")
    };
    let ago = |seconds| Some(SystemTime::now() - Duration::from_secs(seconds));
    i18n::show(&Locale::FALLBACK);
    assert_eq!(detail(change(true, ago(5 * 60 + 5))), "New 5 min ago");
    assert_eq!(detail(change(false, ago(3 * 86_400))), "Changed 3 days ago");
    assert_eq!(detail(change(false, None)), "Changed");
    // A file written after the clock reads has no age.
    let ahead = Some(SystemTime::now() + Duration::from_secs(600));
    assert_eq!(detail(change(true, ahead)), "New");
    i18n::show(&Pseudo::Accented.locale());
    for change in [change(true, ago(30)), change(false, None)] {
        let shown = super::super::render::changed_detail(&change);
        assert!(herdr_i18n::pseudo::is_marked(&shown), "{shown}");
    }
    i18n::show(&Locale::FALLBACK);
}
