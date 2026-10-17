//! Removing an Azure device deletes its machine at Azure by default, with
//! everything it bills for; Remove from Herdr only, beside it, just forgets
//! the device, and the machine keeps billing until it is deleted elsewhere.
use super::{HostMenu, Saved};
use crate::{HerdrWindow, azure::machine::Machine};
use gpui::{prelude::*, *};

impl HostMenu {
    pub(super) fn is_azure(&self) -> bool {
        matches!(
            self.saved,
            Saved::Cloud {
                provider: herdr_client::CloudProvider::Azure,
                ..
            }
        )
    }

    fn noun(&self) -> Option<&'static str> {
        match &self.saved {
            Saved::Cloud { id, .. } if self.is_azure() => Some(Machine::parse(id).noun()),
            _ => None,
        }
    }
}

/// The default button's text: Delete for an Azure device.
pub(super) fn remove_label(host: &HostMenu) -> String {
    match host.noun() {
        Some(noun) => format!("Delete {noun}"),
        None => "Remove".to_owned(),
    }
}

/// What deleting an Azure device does, or `None` for another device.
pub(super) fn removal_note(host: &HostMenu) -> Option<String> {
    let noun = host.noun()?;
    Some(format!(
        "Delete the {noun} {} ({}) at Azure, with its disk and IP, and remove it from this \
         computer? Its files are not kept, and this cannot be undone.",
        host.label, host.target
    ))
}

impl HerdrWindow {
    /// Remove from Herdr only, for an Azure device: the secondary answer.
    pub(super) fn remove_only_button(
        &self,
        host: &HostMenu,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        host.noun()?;
        Some(
            div()
                .id("remove-device-only")
                .debug_selector(|| "remove-device-only".into())
                .p(px(6.))
                .cursor_pointer()
                .child("Remove from Herdr only")
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(host) = &mut this.menu.host {
                        host.destroy = false;
                    }
                    this.confirm_remove_device(window, cx);
                })),
        )
    }
}
