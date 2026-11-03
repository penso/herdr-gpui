use super::super::Account;
use super::*;
use crate::boat::setup::{Key, Overview};

#[gpui::test]
fn recovery_remains_available_after_listing_fails_and_forgetting_requires_confirmation(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.config.boat.api_url = Some(crate::boat::DEFAULT_API_URL.into());
        let card = view.boat_card.as_mut().unwrap();
        card.account = Account::Known(Box::new(Overview {
            key: Ok(Key::Saved),
            sandboxes: Some(Err(crate::boat::Error::StartTimeout)),
            devices: vec![],
            pending: Ok(Some("herdr-pending".into())),
        }));
        card.fields[4].update(cx, |input, cx| input.set_text_selected("7200", cx));
        assert_eq!(card.values(cx).auto_stop, "7200");
        view.forget_boat_pending(cx);
        assert!(view.boat_card.as_ref().unwrap().confirm_forget);
        assert!(
            !view.busy(),
            "first click must not launch a destructive write"
        );
    });
    draw(cx);
    assert!(cx.debug_bounds("cloud-boat-create").is_some());
}
