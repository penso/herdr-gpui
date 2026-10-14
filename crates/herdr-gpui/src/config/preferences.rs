//! Targeted native preference edits, serialized by the managed-config lock.
use super::*;
use herdr_client::protocol::ToastHerdrPosition;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Preference {
    ConfirmCloseTab(bool),
    ConfirmClosePane(bool),
    ConfirmCloseWorkspace(bool),
    RemoveMergedWorktrees(bool),
    ShowSystemLoad(bool),
    AgentCheckpoints(bool),
    ShowListeningPorts(bool),
    ShowSidebarSearch(bool),
    ShowRepositoryIcons(bool),
    ShowDeviceIcons(bool),
    SmoothScroll(bool),
    FocusFollowsMouse(bool),
    NotificationEnabled(Option<bool>),
    NotificationDelay(Option<u64>),
    NotificationPosition(Option<ToastHerdrPosition>),
    /// `[notifications] finished`: GUI-only, so there is no shared value to follow.
    NotificationFinished(bool),
    ClipboardEnabled(Option<bool>),
    ClipboardPosition(Option<ClipboardToastPosition>),
    /// `[terminal] padding_x` or `padding_y`: both sides of one axis.
    TerminalPadding(Axis, Sides),
    /// `[sound] volume`, in percent.
    SoundVolume(u8),
    StatusBar(status_bar::Edit),
    /// `[usage] inline`: whether the sidebar draws the daemon's configured rows.
    UsageInline(bool),
    /// `[usage] hide_providers`: which usage services are turned off.
    UsageProviders(usage_providers::ProviderEdit),
    /// `[updates] channel`: whether the updater also offers beta releases.
    UpdateChannel(UpdateChannel),
}

impl Config {
    pub(crate) fn save_preference(edit: Preference) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_preference_path(edit, &local)
    }

    pub(super) fn save_preference_path(edit: Preference, path: &Path) -> Result<()> {
        let result = (|| -> Result<()> {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            let (table, key, value) = match edit {
                Preference::ConfirmCloseTab(value) => {
                    (None, "confirm_close_tab", Some(value.into()))
                }
                Preference::ConfirmClosePane(value) => {
                    (None, "confirm_close_pane", Some(value.into()))
                }
                Preference::ConfirmCloseWorkspace(value) => {
                    (None, "confirm_close_workspace", Some(value.into()))
                }
                Preference::RemoveMergedWorktrees(value) => {
                    (None, "remove_merged_worktrees", Some(value.into()))
                }
                Preference::ShowSystemLoad(value) => (None, "show_system_load", Some(value.into())),
                Preference::AgentCheckpoints(value) => {
                    (None, "agent_checkpoints", Some(value.into()))
                }
                Preference::ShowListeningPorts(value) => {
                    (None, "show_listening_ports", Some(value.into()))
                }
                Preference::ShowSidebarSearch(value) => {
                    (None, "show_sidebar_search", Some(value.into()))
                }
                Preference::ShowRepositoryIcons(value) => {
                    (None, "show_repository_icons", Some(value.into()))
                }
                Preference::ShowDeviceIcons(value) => {
                    (None, "show_device_icons", Some(value.into()))
                }
                Preference::SmoothScroll(value) => (None, "smooth_scroll", Some(value.into())),
                Preference::FocusFollowsMouse(value) => {
                    (None, "focus_follows_mouse", Some(value.into()))
                }
                Preference::NotificationEnabled(value) => {
                    (Some("notifications"), "enabled", value.map(Into::into))
                }
                Preference::NotificationDelay(value) => {
                    if value.is_some_and(|value| value > 3600) {
                        return Err(crate::herdr_settings::Error::ToastDelay.into());
                    }
                    (
                        Some("notifications"),
                        "delay_seconds",
                        value.map(|value| toml_edit::Value::from(value as i64)),
                    )
                }
                Preference::NotificationPosition(value) => (
                    Some("notifications"),
                    "position",
                    value.map(|value| {
                        toml_edit::Value::from(match value {
                            ToastHerdrPosition::TopLeft => "top-left",
                            ToastHerdrPosition::TopRight => "top-right",
                            ToastHerdrPosition::BottomLeft => "bottom-left",
                            ToastHerdrPosition::BottomRight => "bottom-right",
                        })
                    }),
                ),
                Preference::NotificationFinished(value) => {
                    (Some("notifications"), "finished", Some(value.into()))
                }
                Preference::ClipboardEnabled(value) => {
                    (Some("clipboard_toast"), "enabled", value.map(Into::into))
                }
                Preference::ClipboardPosition(value) => (
                    Some("clipboard_toast"),
                    "position",
                    value.map(|value| {
                        toml_edit::Value::from(match value {
                            ClipboardToastPosition::TopLeft => "top-left",
                            ClipboardToastPosition::TopCenter => "top-center",
                            ClipboardToastPosition::TopRight => "top-right",
                            ClipboardToastPosition::BottomLeft => "bottom-left",
                            ClipboardToastPosition::BottomCenter => "bottom-center",
                            ClipboardToastPosition::BottomRight => "bottom-right",
                        })
                    }),
                ),
                Preference::StatusBar(edit) => {
                    let (key, value) = edit.entry();
                    (Some("status_bar"), key, Some(value))
                }
                Preference::SoundVolume(value) => {
                    if value > SoundConfig::MAX_VOLUME {
                        return Err(Error::InvalidSoundVolume);
                    }
                    (Some("sound"), "volume", Some(i64::from(value).into()))
                }
                Preference::UsageInline(value) => (Some("usage"), "inline", Some(value.into())),
                Preference::UsageProviders(edit) => (
                    Some("usage"),
                    "hide_providers",
                    usage_providers::hide_providers(&document, edit)?,
                ),
                Preference::UpdateChannel(channel) => {
                    (Some("updates"), "channel", Some(channel.as_str().into()))
                }
                Preference::TerminalPadding(axis, sides) => {
                    // The deprecated gap only stands in for an unset
                    // horizontal padding, so the key replacing it removes it
                    // and the file names the value once.
                    if axis == Axis::X
                        && let Some(layout) = document
                            .get_mut("layout")
                            .and_then(toml_edit::Item::as_table_like_mut)
                    {
                        layout.remove("sidebar_gap");
                    }
                    // Whole pixels stay integers, as a person would write them.
                    let pixels = |side: f32| {
                        if side.fract() == 0. {
                            toml_edit::Value::from(side as i64)
                        } else {
                            toml_edit::Value::from(f64::from(side))
                        }
                    };
                    // Sides that agree are one number; a pair names each.
                    let value = if sides.start() == sides.end() {
                        pixels(sides.start())
                    } else {
                        toml_edit::Value::Array(
                            [sides.start(), sides.end()]
                                .map(pixels)
                                .into_iter()
                                .collect(),
                        )
                    };
                    (Some("terminal"), axis.key(), Some(value))
                }
            };
            let target: &mut dyn toml_edit::TableLike = if let Some(table) = table {
                document
                    .entry(table)
                    .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                    .as_table_like_mut()
                    .ok_or(crate::herdr_settings::Error::Table(table))?
            } else {
                document.as_table_mut()
            };
            if let Some(mut value) = value {
                if let Some(previous) = target.get(key).and_then(toml_edit::Item::as_value) {
                    *value.decor_mut() = previous.decor().clone();
                }
                target.insert(key, toml_edit::Item::Value(value));
            } else {
                target.remove(key);
            }
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn targeted_preferences_round_trip_and_reset_without_erasing_unknown_toml() -> anyhow::Result<()>
    {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("local.toml");
        fs::write(
            &path,
            "# personal\nfuture = 'keep'\nlayout = 'orca' # mode\n[notifications]\nfuture = 42\nenabled = false # enabled\n",
        )?;
        for edit in [
            Preference::ConfirmCloseTab(false),
            Preference::ConfirmClosePane(false),
            Preference::ConfirmCloseWorkspace(false),
            Preference::RemoveMergedWorktrees(true),
            Preference::ShowSystemLoad(false),
            Preference::AgentCheckpoints(false),
            Preference::ShowListeningPorts(false),
            Preference::ShowSidebarSearch(false),
            Preference::ShowRepositoryIcons(false),
            Preference::ShowDeviceIcons(false),
            Preference::SmoothScroll(false),
            Preference::NotificationEnabled(Some(true)),
            Preference::NotificationDelay(Some(3600)),
            Preference::NotificationPosition(Some(ToastHerdrPosition::TopLeft)),
            Preference::ClipboardEnabled(Some(false)),
            Preference::ClipboardPosition(Some(ClipboardToastPosition::TopCenter)),
            Preference::TerminalPadding(Axis::X, Sides::new(7.5, 7.5).expect("in band")),
            Preference::TerminalPadding(Axis::Y, Sides::new(4., 6.).expect("in band")),
            Preference::UpdateChannel(UpdateChannel::Beta),
        ] {
            Config::save_preference_path(edit, &path)?;
        }
        let text = fs::read_to_string(&path)?;
        for comment in ["# personal", "# mode", "enabled = true # enabled"] {
            assert!(text.contains(comment));
        }
        let table: toml::Table = toml::from_str(&text)?;
        assert_eq!(table["future"].as_str(), Some("keep"));
        assert_eq!(table["notifications"]["future"].as_integer(), Some(42));
        assert_eq!(table["confirm_close_tab"].as_bool(), Some(false));
        assert_eq!(table["confirm_close_pane"].as_bool(), Some(false));
        assert_eq!(table["confirm_close_workspace"].as_bool(), Some(false));
        assert_eq!(table["remove_merged_worktrees"].as_bool(), Some(true));
        assert_eq!(table["show_system_load"].as_bool(), Some(false));
        assert_eq!(table["agent_checkpoints"].as_bool(), Some(false));
        assert_eq!(table["show_listening_ports"].as_bool(), Some(false));
        assert_eq!(table["show_sidebar_search"].as_bool(), Some(false));
        assert_eq!(table["show_repository_icons"].as_bool(), Some(false));
        assert_eq!(table["show_device_icons"].as_bool(), Some(false));
        assert_eq!(table["smooth_scroll"].as_bool(), Some(false));
        assert_eq!(table["layout"].as_str(), Some("orca"));
        assert_eq!(table["updates"]["channel"].as_str(), Some("beta"));
        assert_eq!(table["terminal"]["padding_x"].as_float(), Some(7.5));
        assert_eq!(
            table["terminal"]["padding_y"],
            toml::Value::Array(vec![4.into(), 6.into()])
        );
        assert_eq!(
            table["notifications"]["delay_seconds"].as_integer(),
            Some(3600)
        );
        assert_eq!(
            table["notifications"]["position"].as_str(),
            Some("top-left")
        );
        assert_eq!(table["clipboard_toast"]["enabled"].as_bool(), Some(false));
        assert_eq!(
            table["clipboard_toast"]["position"].as_str(),
            Some("top-center")
        );
        for edit in [
            Preference::NotificationEnabled(None),
            Preference::NotificationDelay(None),
            Preference::NotificationPosition(None),
            Preference::ClipboardEnabled(None),
            Preference::ClipboardPosition(None),
        ] {
            Config::save_preference_path(edit, &path)?;
        }
        let table: toml::Table = toml::from_str(&fs::read_to_string(&path)?)?;
        assert_eq!(
            table["notifications"].as_table().map(|table| table.len()),
            Some(1)
        );
        assert_eq!(
            table["clipboard_toast"].as_table().map(|table| table.len()),
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn preference_validation_is_atomic_and_preserves_typed_causes() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("local.toml");
        let original = "layout = { mode = 'compact', sidebar_gap = 3 }\n";
        fs::write(&path, original)?;
        let edit = Preference::NotificationDelay(Some(u64::MAX));
        let error = Config::save_preference_path(edit, &path).expect_err("invalid preference");
        assert!(matches!(error, Error::Path { .. }));
        assert!(std::error::Error::source(&error).is_some());
        assert_eq!(fs::read_to_string(&path)?, original);
        let sides = Sides::new(64., 0.).expect("padding in band");
        Config::save_preference_path(Preference::TerminalPadding(Axis::Y, sides), &path)?;
        assert_eq!(
            Config::parse(&fs::read_to_string(&path)?)?
                .terminal_padding
                .y,
            sides
        );
        for original in ["notifications = false\n", "[invalid"] {
            fs::write(&path, original)?;
            assert!(
                Config::save_preference_path(Preference::NotificationEnabled(Some(true)), &path)
                    .is_err()
            );
            assert_eq!(fs::read_to_string(&path)?, original);
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn preference_save_writes_through_a_symlinked_file() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let dotfiles = directory.path().join("dotfiles");
        fs::create_dir(&dotfiles)?;
        let tracked = dotfiles.join("local.toml");
        fs::write(&tracked, "# tracked\n")?;
        let link = directory.path().join("local.toml");
        std::os::unix::fs::symlink(&tracked, &link)?;

        Config::save_preference_path(Preference::ShowSystemLoad(false), &link)?;

        assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
        let text = fs::read_to_string(&tracked)?;
        assert!(text.contains("# tracked"));
        let table: toml::Table = toml::from_str(&text)?;
        assert_eq!(table["show_system_load"].as_bool(), Some(false));
        Ok(())
    }
}
