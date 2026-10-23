use super::*;
use crate::{
    boat::setup::Overview, cloud::SavedDevice, settings_window::cloud_devices::boat::Account,
};

#[gpui::test]
fn unreadable_credentials_keep_saved_devices_visible_and_removable(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(size(px(800.), px(1000.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.boat.api_url = Some(crate::boat::DEFAULT_API_URL.into());
            view.boat_card.as_mut().unwrap().account = Account::Known(Box::new(Overview {
                pending: Ok(None),
                key: Err(crate::boat::Error::Storage(Box::new(crate::Error::GitHub(
                    crate::github::Error::CredentialIo(std::io::ErrorKind::PermissionDenied.into()),
                )))),
                sandboxes: None,
                devices: vec![SavedDevice {
                    provider: CloudProvider::Boat,
                    id: "bx_23456789".into(),
                    label: "Saved sandbox".into(),
                    account: crate::boat::DEFAULT_API_URL.into(),
                    machine: "box".into(),
                    session: "default".into(),
                    enabled: true,
                }],
            }));
            cx.notify();
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("cloud-boat-remove-0").is_some());
    assert!(cx.debug_bounds("cloud-boat-finish-0").is_some());
    view.read_with(cx, |view, _| {
        assert!(!view.busy());
        assert_eq!(view.boat_card.as_ref().unwrap().devices().len(), 1);
    });
}
