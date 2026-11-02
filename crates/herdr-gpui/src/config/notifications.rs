//! Toasts, OS notifications, agent sound volume, and the terminal bell.
use super::Config;
use serde::Deserialize;

/// Where the "copied to clipboard" flash sits, and whether it appears at all.
/// Resolved from the daemon's `[ui.toast.clipboard]`, then from this GUI's own
/// `[clipboard_toast]`, so one terminal preference covers both clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardToast {
    pub enabled: bool,
    pub position: ClipboardToastPosition,
}

impl Default for ClipboardToast {
    fn default() -> Self {
        // herdr's own defaults, so an unconfigured pair of clients agrees.
        Self {
            enabled: true,
            position: ClipboardToastPosition::BottomCenter,
        }
    }
}

/// From herdr src/config/model.rs.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardToastPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    #[default]
    BottomCenter,
    BottomRight,
}

/// What a pane's terminal bell does. Herdr forwards each bell to its
/// foreground client and leaves the reaction to it, as an outer terminal's
/// own bell settings would, so this is the GUI's `[bell]` alone.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct BellConfig {
    /// Ask for attention (bounce the Dock icon) while the window is inactive.
    pub attention: bool,
    /// Play the system alert sound.
    pub sound: bool,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self {
            attention: true,
            sound: false,
        }
    }
}

/// How loud this app plays agent notification sounds. Herdr's `[ui.sound]`
/// has no volume and plays at the file's own level, so this is the GUI's
/// `[sound]` alone and leaves the daemon config untouched.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SoundConfig {
    /// Percent of the sound file's own loudness, inclusive 0..=100.
    #[serde(deserialize_with = "sound_volume")]
    pub volume: u8,
}

impl SoundConfig {
    /// The loudest volume; a percentage above it is refused.
    pub const MAX_VOLUME: u8 = 100;
}

impl Default for SoundConfig {
    fn default() -> Self {
        // The bundled sounds are mastered loud; full level startles.
        Self { volume: 50 }
    }
}

fn sound_volume<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    u8::try_from(u64::deserialize(d)?)
        .ok()
        .filter(|volume| *volume <= SoundConfig::MAX_VOLUME)
        // Part of TOML's parser text, which errors show as written.
        // i18n: data
        .ok_or_else(|| serde::de::Error::custom("sound.volume must be between 0 and 100"))
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct NotificationConfig {
    /// In-app toasts, which take precedence over OS notifications.
    pub enabled: bool,
    /// Shared `system` delivery: post daemon notifications to the OS
    /// notification center. Not a native key, so a local override of
    /// `enabled` decides between the two.
    #[serde(skip)]
    pub system: bool,
    #[serde(deserialize_with = "notification_delay")]
    pub delay_seconds: u64,
    pub position: herdr_client::protocol::ToastHerdrPosition,
    /// Present "finished" notices. A GUI-only key with no shared Herdr
    /// counterpart: turning it off keeps "needs attention" and custom
    /// notifications, so a hook can post its own summary in their place.
    pub finished: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            system: false,
            delay_seconds: 1,
            position: herdr_client::protocol::ToastHerdrPosition::BottomRight,
            finished: true,
        }
    }
}

/// Each key overrides the daemon's answer on its own, so naming one of them
/// here does not silently reset the other to a GUI default.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ClipboardToastSettings {
    enabled: Option<bool>,
    position: Option<ClipboardToastPosition>,
}

impl ClipboardToastSettings {
    /// The GUI's own `enabled` override, if one is set.
    pub(crate) fn enabled(self) -> Option<bool> {
        self.enabled
    }

    /// The GUI's own `position` override, if one is set.
    pub(crate) fn position(self) -> Option<ClipboardToastPosition> {
        self.position
    }

    pub(super) fn resolve(self, base: ClipboardToast) -> ClipboardToast {
        ClipboardToast {
            enabled: self.enabled.unwrap_or(base.enabled),
            position: self.position.unwrap_or(base.position),
        }
    }
}

/// Only explicitly configured GUI keys override the shared Herdr preferences.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct NotificationSettings {
    enabled: Option<bool>,
    #[serde(deserialize_with = "optional_notification_delay")]
    delay_seconds: Option<u64>,
    position: Option<herdr_client::protocol::ToastHerdrPosition>,
    finished: Option<bool>,
}

/// Where a daemon notification that passes the shared policy is presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotificationDelivery {
    Off,
    InApp,
    System,
}

impl NotificationConfig {
    pub(crate) fn delivery(self) -> NotificationDelivery {
        if self.enabled {
            NotificationDelivery::InApp
        } else if self.system {
            NotificationDelivery::System
        } else {
            NotificationDelivery::Off
        }
    }
}

impl NotificationSettings {
    pub(crate) fn enabled(self) -> Option<bool> {
        self.enabled
    }

    pub(crate) fn delay_seconds(self) -> Option<u64> {
        self.delay_seconds
    }

    pub(crate) fn position(self) -> Option<herdr_client::protocol::ToastHerdrPosition> {
        self.position
    }

    pub(super) fn resolve(self, base: NotificationConfig) -> NotificationConfig {
        NotificationConfig {
            enabled: self.enabled.unwrap_or(base.enabled),
            system: base.system,
            delay_seconds: self.delay_seconds.unwrap_or(base.delay_seconds),
            position: self.position.unwrap_or(base.position),
            finished: self.finished.unwrap_or(base.finished),
        }
    }
}

fn optional_notification_delay<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<u64>, D::Error> {
    notification_delay(d).map(Some)
}

fn notification_delay<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let seconds = u64::deserialize(d)?;
    if seconds > 3600 {
        // Part of TOML's parser text, which errors show as written.
        return Err(serde::de::Error::custom(
            // i18n: data
            "notifications.delay_seconds must be between 0 and 3600",
        ));
    }
    Ok(seconds)
}

impl Config {
    /// Pure application of a prepared shared snapshot. Native explicit keys win;
    /// system delivery posts OS notifications instead of in-app toasts, and
    /// terminal delivery has no outer terminal to reach from this GUI.
    pub(crate) fn apply_shared_notifications(&mut self, shared: &crate::herdr_settings::Settings) {
        self.notifications = self.notification_overrides.resolve(NotificationConfig {
            enabled: shared.toast_delivery == crate::herdr_settings::ToastDelivery::Herdr,
            system: shared.toast_delivery == crate::herdr_settings::ToastDelivery::System,
            delay_seconds: shared.toast_delay_seconds,
            position: shared.toast_position,
            ..NotificationConfig::default()
        });
    }
}
