//! The Phone notifications card: opt-in, per-event switches, and a test send.
//! Credentials are only read from the config file, never typed or shown here.
use super::*;
#[cfg(test)]
use crate::notifications::phone::Service;
use crate::{config::preferences::Preference, notifications::phone::Message};

/// The last test send, shown under its button.
pub(super) enum PhoneTest {
    Sending,
    Sent,
    Failed(crate::Error),
}

#[cfg(test)]
pub(super) type PhoneSend = fn(&Service, &Message) -> crate::Result<()>;

impl SettingsWindow {
    pub(super) fn phone_controls(&self, cx: &mut Context<Self>) -> Div {
        let phone = &self.config.phone;
        let service = phone.service();
        let configured = service.is_ok();
        let destination = match &service {
            Ok(service) => format!("{} ({})", service.name(), service.host()),
            Err(error) => error.to_string(),
        };
        let testing = matches!(self.controls.phone_test, Some(PhoneTest::Sending));
        let can_test = configured && !testing;
        let mut card = self
            .control_card("Phone notifications")
            .debug_selector(|| "settings-phone".into())
            .child(self.control_note(
                "Forward agent alerts to ntfy or Pushover while this window is not focused. Only the alert title and a short summary are sent.",
            ))
            .child(self.control_row("Service", destination))
            .child(self.preference_switch(
                "settings-phone-enabled",
                "Forward to phone",
                phone.enabled,
                Preference::PhoneEnabled(!phone.enabled),
                cx,
            ))
            .child(self.preference_switch(
                "settings-phone-blocked",
                "Agent needs attention",
                phone.blocked,
                Preference::PhoneBlocked(!phone.blocked),
                cx,
            ))
            .child(self.preference_switch(
                "settings-phone-done",
                "Agent finished",
                phone.done,
                Preference::PhoneDone(!phone.done),
                cx,
            ))
            .child(
                self.control_choice(
                    "settings-phone-test",
                    if testing { "Sending..." } else { "Send test" },
                    false,
                    can_test,
                )
                .debug_selector(|| "settings-phone-test".into())
                .when(can_test, |button| {
                    button.on_click(cx.listener(|this, _, _, cx| this.send_phone_test(cx)))
                }),
            );
        if let Some(result) = &self.controls.phone_test {
            card = card.child(self.control_note(match result {
                PhoneTest::Sending => "Sending a test notification...".to_owned(),
                PhoneTest::Sent => "Test notification sent.".to_owned(),
                PhoneTest::Failed(error) => format!("Test failed: {error}"),
            }));
        }
        card.child(self.control_note(format!(
            "Set the service under [phone] in {}.",
            self.controls.local_path
        )))
    }

    /// Sends one test message on the background executor, bypassing the rate
    /// limit and the focus rule so the setup can be checked from here.
    fn send_phone_test(&mut self, cx: &mut Context<Self>) {
        if matches!(self.controls.phone_test, Some(PhoneTest::Sending)) {
            return;
        }
        let service = match self.config.phone.service() {
            Ok(service) => service,
            Err(error) => {
                self.controls.phone_test = Some(PhoneTest::Failed(error));
                cx.notify();
                return;
            }
        };
        #[cfg(test)]
        let send = self
            .controls
            .phone_send
            .unwrap_or(crate::notifications::phone::send);
        #[cfg(not(test))]
        let send = crate::notifications::phone::send;
        self.controls.phone_test = Some(PhoneTest::Sending);
        let work = cx
            .background_executor()
            .spawn(async move { send(&service, &Message::test()) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.controls.phone_test = Some(match result {
                    Ok(()) => PhoneTest::Sent,
                    Err(error) => PhoneTest::Failed(error),
                });
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
