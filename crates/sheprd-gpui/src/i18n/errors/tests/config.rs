use super::*;
use crate::config::{Error, ThemeParseError, ssh_hosts, tab_layout};
use std::path::PathBuf;

fn parse_int() -> std::num::ParseIntError {
    "x".parse::<u8>().unwrap_err()
}

#[test]
fn every_config_variant_has_a_message() {
    assert_localized(&[
        Error::DeviceTheme {
            device: "ssh-build-box".into(),
            source: Box::new(crate::Error::NoTab),
        },
        Error::EmptyFontFallback("font"),
        Error::EmptyFontFamily("font"),
        Error::EmptyTheme,
        Error::InvalidChatWidth,
        Error::InvalidCodeTable,
        Error::InvalidDeviceId("nope".into()),
        Error::InvalidDevicesTable,
        Error::InvalidFavoriteThemes,
        Error::InvalidFontSize("zen"),
        Error::InvalidFontWeight("font"),
        Error::InvalidHostColor {
            host: "build-box".into(),
            value: "blue".into(),
        },
        Error::InvalidLineHeight("font"),
        Error::InvalidSidebarGap,
        Error::InvalidSidebarMetric {
            key: "row_gap",
            max: 64.0,
        },
        Error::InvalidSidebarMetric {
            key: "indent",
            max: 12.5,
        },
        Error::InvalidSoundVolume,
        Error::InvalidTerminalPadding("padding"),
        Error::InvalidThemePair,
        Error::InvalidThemePath,
        Error::InvalidUsageTable,
        Error::InvalidZenFontSize,
        Error::InvalidZenWidth,
        Error::KeyNotTable {
            key: "sidebar.rows".into(),
        },
        Error::MigrationConflict {
            original: PathBuf::from("/home/me/.config/herdr/config-gpui.toml"),
            local: PathBuf::from("/home/me/.config/herdr/config-gpui.local.toml"),
        },
        Error::PreferencesNotObject,
        Error::ThemeLine {
            line: 12,
            key: "background".into(),
            source: ThemeParseError::InvalidColor,
        },
        Error::ThemeNotFound {
            name: "Dracula".into(),
            directories: vec![PathBuf::from("/a/themes"), PathBuf::from("/b/themes")],
        },
        Error::TooManyDevices(64),
        Error::TooManyFontFallbacks("font"),
        Error::UnknownKeys(vec!["colour".into()]),
        Error::UnknownKeys(vec!["colour".into(), "fnot.size".into()]),
        Error::UnknownLayout("columns".into()),
    ]);
}

#[test]
fn every_theme_parse_variant_has_a_message() {
    assert_localized(&[
        ThemeParseError::InvalidColor,
        ThemeParseError::InvalidHex(parse_int()),
        ThemeParseError::MissingPaletteColor,
        ThemeParseError::InvalidPaletteIndex(parse_int()),
        ThemeParseError::PaletteIndexOutOfRange,
    ]);
}

#[test]
fn every_ssh_hosts_variant_has_a_message() {
    assert_localized(&[
        ssh_hosts::Error::InvalidTable,
        ssh_hosts::Error::InvalidTarget("-oProxyCommand=x".into()),
        ssh_hosts::Error::TooMany(256),
    ]);
}

#[test]
fn every_tab_layout_variant_has_a_message() {
    assert_localized(&[tab_layout::Error::Unknown("ribbons".into())]);
}
