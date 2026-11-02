#![allow(clippy::unwrap_used)]
use super::*;
use crate::config::{Config, Theme};
use core::prelude::v1::test;
use std::{cell::RefCell, rc::Rc};

#[test]
fn hex_input_is_lenient_on_case_space_and_short_form() {
    assert_eq!(parse_hex_input("#3a4a6b"), Some(0x3a4a6b));
    assert_eq!(parse_hex_input("3A4A6B"), Some(0x3a4a6b));
    assert_eq!(parse_hex_input(" #abc "), Some(0xaabbcc));
    assert_eq!(parse_hex_input("#12345"), None);
    assert_eq!(parse_hex_input("xyz"), None);
    assert_eq!(parse_hex_input(""), None);
    assert_eq!(parse_hex_input("+a1b2c"), None);
    assert_eq!(parse_hex_input("##abc"), None);
}

struct Harness {
    seen: Rc<RefCell<Vec<HostColorChange>>>,
    current: Option<u32>,
    host: &'static str,
    width: Option<f32>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let seen = self.seen.clone();
        let row = swatch_row(
            &Theme::default(),
            self.host,
            0,
            self.current,
            &Config::default().terminal,
            SharedString::default(),
            None,
            move |change, _, _| seen.borrow_mut().push(change),
            |_, _| {},
        );
        let container = div().debug_selector(|| "container".into()).child(row);
        div().size_full().child(match self.width {
            Some(width) => container.w(px(width)),
            None => container.w_full(),
        })
    }
}

#[gpui::test]
fn swatches_set_none_clears_and_reset_clears(cx: &mut TestAppContext) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|_, _| Harness {
        seen: seen.clone(),
        current: Some(0x3a4a6b),
        host: "Personal",
        width: None,
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for selector in [
        "host-colour-0-swatch-4",
        "host-colour-0-none",
        "host-colour-0-reset",
    ] {
        let center = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(center, Default::default());
    }
    cx.run_until_parked();
    assert_eq!(
        *seen.borrow(),
        vec![
            HostColorChange::Set(SWATCHES[4]),
            HostColorChange::Clear,
            HostColorChange::Clear
        ]
    );
}

#[gpui::test]
fn reset_is_absent_without_a_colour(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Harness {
        seen: Rc::default(),
        current: None,
        host: "Personal",
        width: None,
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("host-colour-0-reset").is_none());
    assert!(cx.debug_bounds("host-colour-0-hex").is_some());
}

/// At the window's minimum width the page body is about 440 px; a long host
/// name and the full strip must still fit, the strip wrapping instead.
#[gpui::test]
fn a_long_host_fits_a_narrow_body(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Harness {
        seen: Rc::default(),
        current: Some(0x3a4a6b),
        host: "a-very-long-host-name.example.internal.x",
        width: Some(440.),
    });
    cx.simulate_resize(size(px(680.), px(400.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let container = cx.debug_bounds("container").unwrap();
    assert_eq!(f32::from(container.size.width), 440.);
    for selector in ["host-colour-0-hex", "host-colour-0-reset"] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(
            bounds.left() >= container.left() && bounds.right() <= container.right(),
            "{selector} {bounds:?} outside {container:?}"
        );
    }
    let label = cx.debug_bounds("host-colour-0-label").unwrap();
    assert!(f32::from(label.size.width) <= 120., "{label:?}");
    assert!(label.right() <= container.right());
}
