//! Targeted native preference edits, serialized by the managed-config lock.
use super::*;
use herdr_client::protocol::ToastHerdrPosition;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Preference {
    AgentsPlacement(AgentsPlacement),
    /// `[chat] glide`: whether chats scroll smoothly to new output.
    ChatGlide(bool),
    /// `[chat] launcher`: where tab strips offer one-click chats.
    ChatLauncher(ChatLauncher),
    /// `[chat] layout`: how chat views draw.
    ChatLayout(ChatLayoutMode),
    /// `[chat] max_width`: the reading column, or zero for full width.
    ChatMaxWidth(u16),
    ConfirmCloseTab(bool),
    ConfirmClosePane(bool),
    ConfirmCloseWorkspace(bool),
    RemoveMergedWorktrees(bool),
    ShowSystemLoad(bool),
    ShowNetworkQuality(bool),
    ShowGpuLoad(bool),
    ShowSidebarGpuLoad(bool),
    AgentCheckpoints(bool),
    ShowListeningPorts(bool),
    ShowAgentsHeader(bool),
    ShowSidebarSearch(bool),
    ShowRepositoryIcons(bool),
    ShowWindowControls(bool),
    /// `status_icons`: the set agent status is drawn with.
    StatusIcons(StatusIcons),
    /// `tab_layout`: how tab strips draw their tabs.
    TabLayout(TabLayoutMode),
    /// `edge_rail`: the bar on busy and finished rows.
    EdgeRail(bool),
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
    /// One `[remote]` key.
    Remote(remote::Edit),
    /// `[usage] inline`: whether the sidebar draws the daemon's configured rows.
    UsageInline(bool),
    /// `[updates] channel`: whether the updater also offers beta releases.
    UpdateChannel(UpdateChannel),
    /// One `[features]` flag.
    Feature(Feature, bool),
    /// `None` removes the override so the layout preset's value applies.
    SidebarIndent(Option<f32>),
    SidebarRowPadding(Option<f32>),
    SidebarRowGap(Option<f32>),
    SidebarHostGap(Option<f32>),
    SidebarSelect(SelectMode),
    /// `[sidebar] activity_bar`: whether the icon column leads the window.
    SidebarActivityBar(bool),
    /// `[sidebar] automations`: whether the bar shows Automations.
    SidebarAutomations(bool),
    /// `[sidebar] device_footer`: when the sidebar draws its device footer.
    SidebarDeviceFooter(DeviceFooter),
    /// `[sidebar] change_counts`: whose line counts a worktree row shows.
    SidebarChangeCounts(ChangeCounts),
    /// `None` removes the host's colour.
    HostColor {
        host: String,
        color: Option<u32>,
    },
}

/// A `[sidebar]` metric key, checked against the parser's band table.
fn sidebar_metric(
    key: &'static str,
    value: Option<f32>,
) -> Result<(Option<&'static str>, &'static str, Option<toml_edit::Value>)> {
    if let Some(value) = value {
        let max = SidebarOverrides::BANDS
            .iter()
            .find_map(|(name, max)| (*name == key).then_some(*max))
            .unwrap_or(0.);
        if !value.is_finite() || !(0.0..=max).contains(&value) {
            return Err(Error::InvalidSidebarMetric { key, max }.into());
        }
    }
    Ok((
        Some("sidebar"),
        key,
        value.map(|value| toml_edit::Value::from(f64::from(value))),
    ))
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
                Preference::AgentsPlacement(placement) => {
                    (None, "agents_placement", Some(placement.as_str().into()))
                }
                Preference::ChatGlide(value) => (Some("chat"), "glide", Some(value.into())),
                Preference::ChatLauncher(launcher) => {
                    (Some("chat"), "launcher", Some(launcher.name().into()))
                }
                Preference::ChatLayout(mode) => (Some("chat"), "layout", Some(mode.name().into())),
                Preference::ChatMaxWidth(width) => {
                    chat::check_max_width(width)?;
                    (Some("chat"), "max_width", Some(i64::from(width).into()))
                }
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
                Preference::ShowNetworkQuality(value) => {
                    (None, "show_network_quality", Some(value.into()))
                }
                Preference::ShowGpuLoad(value) => (None, "show_gpu_load", Some(value.into())),
                Preference::ShowSidebarGpuLoad(value) => {
                    (None, "show_sidebar_gpu_load", Some(value.into()))
                }
                Preference::AgentCheckpoints(value) => {
                    (None, "agent_checkpoints", Some(value.into()))
                }
                Preference::ShowListeningPorts(value) => {
                    (None, "show_listening_ports", Some(value.into()))
                }
                Preference::ShowAgentsHeader(value) => {
                    (None, "show_agents_header", Some(value.into()))
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
                Preference::ShowWindowControls(value) => {
                    (None, "show_window_controls", Some(value.into()))
                }
                Preference::SmoothScroll(value) => (None, "smooth_scroll", Some(value.into())),
                Preference::StatusIcons(value) => {
                    (None, "status_icons", Some(value.as_str().into()))
                }
                Preference::EdgeRail(value) => (None, "edge_rail", Some(value.into())),
                Preference::TabLayout(mode) => (None, "tab_layout", Some(mode.name().into())),
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
                Preference::SidebarIndent(value) => sidebar_metric("indent", value)?,
                Preference::SidebarRowPadding(value) => sidebar_metric("row_padding", value)?,
                Preference::SidebarRowGap(value) => sidebar_metric("gap", value)?,
                Preference::SidebarHostGap(value) => sidebar_metric("host_gap", value)?,
                Preference::SidebarSelect(mode) => (
                    Some("sidebar"),
                    "select",
                    Some(toml_edit::Value::from(match mode {
                        SelectMode::Row => "row",
                        SelectMode::Group => "group",
                        SelectMode::GroupDim => "group-dim",
                    })),
                ),
                Preference::SidebarActivityBar(value) => {
                    (Some("sidebar"), "activity_bar", Some(value.into()))
                }
                Preference::SidebarAutomations(value) => {
                    (Some("sidebar"), "automations", Some(value.into()))
                }
                Preference::SidebarDeviceFooter(footer) => (
                    Some("sidebar"),
                    "device_footer",
                    Some(toml_edit::Value::from(footer.as_str())),
                ),
                Preference::SidebarChangeCounts(counts) => (
                    Some("sidebar"),
                    "change_counts",
                    Some(toml_edit::Value::from(counts.as_str())),
                ),
                Preference::HostColor { host, color } => {
                    if let Some(color) = color.filter(|color| *color > 0xFF_FFFF) {
                        return Err(Error::InvalidHostColor {
                            host,
                            value: format!("#{color:x}"),
                        }
                        .into());
                    }
                    let sidebar = document
                        .entry("sidebar")
                        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                        .as_table_like_mut()
                        .ok_or(crate::herdr_settings::Error::Table("sidebar"))?;
                    let hosts = sidebar
                        .entry("hosts")
                        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                        .as_table_like_mut()
                        .ok_or(crate::herdr_settings::Error::Table("sidebar.hosts"))?;
                    match color {
                        Some(color) => {
                            let mut value = toml_edit::Value::from(format!("#{color:06x}"));
                            // A comment on the old colour stays with the new one.
                            if let Some(previous) =
                                hosts.get(&host).and_then(toml_edit::Item::as_value)
                            {
                                *value.decor_mut() = previous.decor().clone();
                            }
                            hosts.insert(&host, toml_edit::Item::Value(value));
                        }
                        None => {
                            hosts.remove(&host);
                        }
                    }
                    return write_config(path, &document.to_string());
                }
                Preference::StatusBar(edit) => {
                    let (key, value) = edit.entry();
                    (Some("status_bar"), key, Some(value))
                }
                Preference::Remote(edit) => {
                    let (key, value) = edit.entry()?;
                    (Some("remote"), key, Some(value))
                }
                Preference::SoundVolume(value) => {
                    if value > SoundConfig::MAX_VOLUME {
                        return Err(Error::InvalidSoundVolume.into());
                    }
                    (Some("sound"), "volume", Some(i64::from(value).into()))
                }
                Preference::UsageInline(value) => (Some("usage"), "inline", Some(value.into())),
                Preference::UpdateChannel(channel) => {
                    (Some("updates"), "channel", Some(channel.as_str().into()))
                }
                Preference::Feature(feature, value) => {
                    (Some("features"), feature.key(), Some(value.into()))
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
            Preference::ShowNetworkQuality(true),
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
            Preference::Feature(Feature::SidebarHoverMenu, true),
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
        assert_eq!(table["show_network_quality"].as_bool(), Some(true));
        assert_eq!(table["agent_checkpoints"].as_bool(), Some(false));
        assert_eq!(table["show_listening_ports"].as_bool(), Some(false));
        assert_eq!(table["show_sidebar_search"].as_bool(), Some(false));
        assert_eq!(table["show_repository_icons"].as_bool(), Some(false));
        assert_eq!(table["show_device_icons"].as_bool(), Some(false));
        assert_eq!(table["smooth_scroll"].as_bool(), Some(false));
        assert_eq!(table["layout"].as_str(), Some("orca"));
        assert_eq!(table["updates"]["channel"].as_str(), Some("beta"));
        assert_eq!(
            table["features"]["sidebar_hover_menu"].as_bool(),
            Some(true)
        );
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
        assert!(matches!(error, crate::Error::Path { .. }));
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
