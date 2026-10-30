use super::*;
use herdr_i18n::{Locale, Pseudo};

fn english(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn steps() -> Vec<Step> {
    vec![
        Step::Creating,
        Step::Connecting,
        Step::Starting,
        Step::Building("pending".into()),
        Step::CheckingHerdr,
        Step::Image(Progress {
            step: Some((2, 4)),
            status: "RUN make".into(),
        }),
        Step::InstallingHerdr,
    ]
}

#[test]
fn steps_read_as_short_status_words() {
    crate::i18n::show(&Locale::FALLBACK);
    let shown: Vec<_> = steps().iter().map(|one| english(step(one))).collect();
    assert_eq!(
        shown,
        [
            "Creating…",
            "Connecting…",
            "Starting…",
            "Building (pending)…",
            "Checking for Herdr…",
            "Building the image: Step 2 of 4: RUN make",
            "Installing Herdr…",
        ]
    );
    let quiet = Progress {
        step: None,
        status: "building".into(),
    };
    assert_eq!(build(&quiet), "building");
}

#[test]
fn every_step_is_a_message_under_en_xa() {
    crate::i18n::show(&Pseudo::Accented.locale());
    let mut shown: Vec<_> = steps().iter().map(step).collect();
    if let Some(&provider) = CloudProvider::ALL.first() {
        shown.push(contacting(provider));
    }
    crate::i18n::show(&Locale::FALLBACK);
    for line in shown {
        assert!(herdr_i18n::pseudo::is_marked(&line), "{line}");
    }
}
