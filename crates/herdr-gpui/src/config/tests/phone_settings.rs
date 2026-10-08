use super::*;

#[test]
fn phone_settings_feed_the_notification_policy_and_survive_shared_reloads() -> anyhow::Result<()> {
    use crate::herdr_settings::Settings as Shared;
    use crate::notifications::phone::PhoneEvents;

    assert_eq!(
        Config::parse("")?.notifications.phone,
        PhoneEvents::default()
    );
    // Opting in without a service forwards nothing.
    assert_eq!(
        Config::parse("[phone]\nenabled = true")?
            .notifications
            .phone,
        PhoneEvents::default()
    );
    let mut config =
        Config::parse("[phone]\nenabled = true\ndone = false\n[phone.ntfy]\ntopic = 'herdr-x'")?;
    let expected = PhoneEvents {
        blocked: true,
        done: false,
    };
    assert_eq!(config.notifications.phone, expected);
    config.apply_shared_notifications(&Shared::parse_text("")?);
    assert_eq!(config.notifications.phone, expected);
    for text in [
        "[phone.ntfy]\ntopic = 'has space'",
        "[phone.ntfy]\nserver = 'ftp://x'",
        "[phone]\nunknown = 1",
    ] {
        assert!(Config::parse(text).is_err(), "{text}");
    }
    Ok(())
}
