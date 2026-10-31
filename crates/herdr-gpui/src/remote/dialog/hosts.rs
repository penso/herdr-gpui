//! The other machines this app connects to, ticked in the approval step:
//! allowing the phone also allows it on each ticked machine, which the
//! phone then pairs with directly (`remote::links`), so it keeps them while
//! this computer sleeps.

use super::*;
use crate::{notifications::safe_text, toggles::checkbox};
use herdr_remote_control::control::hosts::{Host, HostId};

/// A machine in the list, and whether it is ticked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub(crate) host: Host,
    pub(crate) ticked: bool,
}

/// Every machine, ticked: the phone should reach them all.
pub(crate) fn choices(hosts: Vec<Host>) -> Vec<Choice> {
    hosts
        .into_iter()
        .map(|host| Choice { host, ticked: true })
        .collect()
}

/// The ticked machines' ids, for the approval.
pub(crate) fn ticked(choices: &[Choice]) -> Vec<HostId> {
    choices
        .iter()
        .filter(|choice| choice.ticked)
        .map(|choice| choice.host.id.clone())
        .collect()
}

impl HerdrWindow {
    pub(super) fn toggle_pairing_host(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(choice) = self
            .menu
            .remote_pairing
            .as_mut()
            .and_then(|dialog| dialog.hosts.get_mut(index))
        else {
            return;
        };
        choice.ticked = !choice.ticked;
        cx.notify();
    }

    /// The list under the matching code, or nothing when this app connects
    /// to no other machine.
    pub(super) fn pairing_hosts(&self, enabled: bool, cx: &mut Context<Self>) -> Option<Div> {
        let dialog = self.menu.remote_pairing.as_ref()?;
        if dialog.hosts.is_empty() {
            return None;
        }
        let theme = &self.theme;
        let rows = dialog.hosts.iter().enumerate().map(|(index, choice)| {
            let name = safe_text(&choice.host.name, 64);
            div()
                .id(("remote-pair-host", index))
                .debug_selector(move || format!("remote-pair-host-{index}"))
                .role(Role::CheckBox)
                .aria_label(SharedString::from(name.clone()))
                .aria_toggled(if choice.ticked {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(4.))
                .py(px(3.))
                .rounded(px(crate::config::corners::CONTROL))
                .when(enabled, |row| {
                    row.cursor_pointer()
                        .hover(|row| row.bg(rgb(theme.active)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_pairing_host(index, cx);
                        }))
                })
                .when(!enabled, |row| row.opacity(0.5))
                .child(checkbox(theme, 14., choice.ticked))
                .child(div().min_w_0().truncate().child(name))
        });
        Some(
            div()
                .debug_selector(|| "remote-pair-hosts".into())
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(div().text_color(rgb(theme.subtext())).child(
                    "Also pair it with these machines, so it reaches them while this computer \
                     sleeps:",
                ))
                .children(rows),
        )
    }
}
