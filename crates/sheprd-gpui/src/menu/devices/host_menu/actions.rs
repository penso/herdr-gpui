//! The rows each kind of device's menu offers.
use super::tags::Mode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Rename,
    /// Toggles the device's mute, which silences everything on it.
    Mute,
    /// Toggles `[devices.<id>] keybindings` between local and server.
    ServerKeybindings,
    /// Forwards one of the host's listening ports to this computer.
    #[cfg(unix)]
    ForwardPort,
    /// Picks `[devices.<id>] theme`, the terminal's colors on this device.
    Theme,
    /// Picks the section the device lists under, or the tags it also carries.
    Tags(Mode),
    /// Opens Settings > Cloud devices on the device's provider.
    #[cfg(feature = "cloud")]
    ProviderSettings,
    /// Runs the configured setup script on the cloud device again.
    #[cfg(feature = "cloud")]
    RunSetup,
    Remove,
}

/// Each row's action and the message naming it.
pub(super) const SSH_ACTIONS: &[(Action, &str)] = &[
    (Action::Rename, "host-menu-rename"),
    (Action::Tags(Mode::MoveTo), "device-tags-move-to"),
    (Action::Tags(Mode::AlsoTagged), "device-tags-also-tagged"),
    (Action::Mute, "host-menu-mute"),
    (Action::ServerKeybindings, "host-menu-server-keybindings"),
    // Port forwards need an `ssh` control master, which Windows OpenSSH lacks.
    #[cfg(unix)]
    (Action::ForwardPort, "host-menu-forward-port"),
    (Action::Theme, "host-menu-theme"),
    (Action::Remove, "host-menu-remove"),
];

/// Port forwards, server keybindings, and the terminal theme go through the
/// SSH profile, which a cloud device does not have.
#[cfg(feature = "cloud")]
pub(super) const CLOUD_ACTIONS: &[(Action, &str)] = &[
    (Action::Rename, "host-menu-rename"),
    (Action::Tags(Mode::MoveTo), "device-tags-move-to"),
    (Action::Tags(Mode::AlsoTagged), "device-tags-also-tagged"),
    (Action::Mute, "host-menu-mute"),
    (Action::ProviderSettings, "host-menu-cloud-settings"),
    (Action::RunSetup, "host-menu-run-setup"),
    (Action::Remove, "host-menu-remove"),
];

/// This computer is not saved anywhere, so it can only be grouped and muted.
pub(super) const LOCAL_ACTIONS: &[(Action, &str)] = &[
    (Action::Tags(Mode::MoveTo), "device-tags-move-to"),
    (Action::Tags(Mode::AlsoTagged), "device-tags-also-tagged"),
    (Action::Mute, "host-menu-mute"),
];
