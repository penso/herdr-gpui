//! Windows toasts whose click reaches the app through a `herdr-gpui://`
//! link. GPUI's own toasts answer a click only inside the process that
//! posted them, and only while it still holds the toast, so a click in
//! Action Center later, or one Windows delivers without letting the app take
//! the foreground, is lost. These toasts use protocol activation instead: the
//! shell starts `herdr-gpui.exe herdr-gpui://notification?tag=TAG`, which
//! hands the link to the running app and passes the foreground on (see
//! `link`). They are used once the installer has registered the scheme;
//! until then, GPUI's toasts post as before.

#[cfg(test)]
mod tests;

use crate::{constants::APP_ID, link::Link};
use gpui::SystemNotification;
use windows::{
    Data::Xml::Dom::XmlDocument,
    UI::Notifications::{ToastNotification, ToastNotificationManager, ToastNotifier},
    core::{HSTRING, h},
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Windows refused the notification")]
    Toast(#[from] windows::core::Error),
}

/// What the app learns, off the UI thread, before its first toast.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Shell {
    /// Whether `herdr-gpui://` links start this app for the user.
    pub links: bool,
}

/// Checks the link handler and names the app to the notification center, as
/// GPUI does before its first toast. Registry access: run it off the UI
/// thread.
pub(crate) fn prepare() -> Shell {
    let user = windows_registry::CURRENT_USER;
    let links = user
        .open(r"Software\Classes\herdr-gpui\shell\open\command") // i18n: data
        .and_then(|key| key.get_string(""))
        .is_ok_and(|command| !command.trim().is_empty());
    // Without this key, or a Start menu shortcut carrying the ID, an
    // unpackaged app's toasts are dropped.
    if let Err(error) = user
        // i18n: data
        .create(format!(r"Software\Classes\AppUserModelId\{APP_ID}"))
        // i18n: data
        .and_then(|key| key.set_string("DisplayName", crate::constants::WINDOW_TITLE))
    {
        tracing::warn!(%error, "Notifications may not show: the app's name is not registered");
    }
    Shell { links }
}

/// Posts toasts, keeping the notifier once made.
#[derive(Default)]
pub(crate) struct Toasts {
    notifier: Option<ToastNotifier>,
}

impl Toasts {
    pub(crate) fn show(&mut self, notification: &SystemNotification) -> Result<(), Error> {
        let notifier = match &self.notifier {
            Some(notifier) => notifier.clone(),
            None => {
                let notifier =
                    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?;
                self.notifier = Some(notifier.clone());
                notifier
            }
        };
        let toast = ToastNotification::CreateToastNotification(&document(notification)?)?;
        // A newer toast for the same pane replaces the older one, as GPUI's
        // do; Windows caps tags at 64 characters.
        toast.SetTag(&HSTRING::from(short_tag(&notification.tag)))?;
        notifier.Show(&toast)?;
        Ok(())
    }
}

/// The toast's XML. Text goes in through the DOM, never as markup, so a
/// notification's text cannot add elements or attributes.
fn document(notification: &SystemNotification) -> windows::core::Result<XmlDocument> {
    let document = XmlDocument::new()?;
    let toast = document.CreateElement(h!("toast"))?;
    let link = Link::Notification {
        tag: notification.tag.to_string(),
    };
    toast.SetAttribute(h!("launch"), &HSTRING::from(link.to_url()))?;
    toast.SetAttribute(h!("activationType"), h!("protocol"))?;
    document.AppendChild(&toast)?;
    let visual = document.CreateElement(h!("visual"))?;
    toast.AppendChild(&visual)?;
    let binding = document.CreateElement(h!("binding"))?;
    binding.SetAttribute(h!("template"), h!("ToastGeneric"))?; // i18n: data
    visual.AppendChild(&binding)?;
    for text in [&notification.title, &notification.body] {
        let element = document.CreateElement(h!("text"))?;
        element.SetInnerText(&HSTRING::from(text.as_ref()))?;
        binding.AppendChild(&element)?;
    }
    // Silent: sounds follow the app's own settings.
    let audio = document.CreateElement(h!("audio"))?;
    audio.SetAttribute(h!("silent"), h!("true"))?;
    toast.AppendChild(&audio)?;
    Ok(document)
}

/// A fixed-width tag: FNV-1a, stable across builds.
pub(crate) fn short_tag(tag: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in tag.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}
