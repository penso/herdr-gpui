use super::*;
use crate::{i18n, listening_ports::render::hint};
use herdr_i18n::{Locale, Pseudo};

#[test]
fn every_port_hint_is_a_message_under_en_xa() {
    let named = Port {
        number: 3000,
        bind: Bind::Loopback,
        process: "node".into(),
    };
    let unnamed = Port {
        process: String::new(),
        ..named.clone()
    };
    let tunnel = Link::Tunnel(Key {
        target: "devbox".into(),
        address: None,
        port: 3000,
    });
    i18n::show(&Pseudo::Accented.locale());
    let shown = [
        hint(&named, None),
        hint(&unnamed, Some(&tunnel)),
        hint(&named, named.link(&Origin::Local).as_ref()),
    ];
    i18n::show(&Locale::FALLBACK);
    for line in shown.iter().flat_map(|hint| hint.lines()) {
        assert!(herdr_i18n::pseudo::is_marked(line), "{line}");
    }
}
