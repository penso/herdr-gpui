//! A page's toolbar under `en-XB`: it runs from the right, so Back sits at
//! the start, pointing toward it, and the address field follows.
use super::*;
use crate::controls::Command;
use herdr_i18n::Pseudo;

#[gpui::test]
fn the_toolbar_runs_from_the_right(cx: &mut gpui::TestAppContext) {
    crate::i18n::show(&Pseudo::Bidi.locale());
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::NewBrowserTab, window, cx)
        })
    });
    draw(cx);
    let back = cx.debug_bounds("browser-back").unwrap();
    let forward = cx.debug_bounds("browser-forward").unwrap();
    let address = cx.debug_bounds("browser-address").unwrap();
    assert!(back.left() > forward.right(), "{back:?} {forward:?}");
    assert!(forward.left() > address.right(), "{forward:?} {address:?}");
    assert_eq!(
        crate::direction::icon("icons/arrow-left.svg"),
        "icons/arrow-right.svg"
    );
    crate::i18n::show(&herdr_i18n::Locale::FALLBACK);
}
