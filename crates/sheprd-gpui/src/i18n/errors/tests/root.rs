use super::*;
use crate::Error;
use std::{io, path::PathBuf, sync::Arc};

fn io() -> io::Error {
    io::Error::other("disk on fire")
}

fn image() -> image::ImageError {
    image::ImageError::IoError(io())
}

fn json() -> serde_json::Error {
    serde_json::from_str::<u8>("x").unwrap_err()
}

#[test]
fn every_own_variant_has_a_message() {
    let path = PathBuf::from("/tmp/notes.md");
    let mut errors = vec![
        Error::AgentContext {
            operation: "create",
            source: io(),
        },
        Error::AgentContext {
            operation: "write",
            source: io(),
        },
        Error::Bonjour(mdns_sd::Error::Msg("no interface".into())),
        Error::BundleExists { path: path.clone() },
        Error::BundleWrite {
            path: path.clone(),
            source: io(),
        },
        Error::Cleanup {
            source: io(),
            path: path.clone(),
            cleanup: io(),
        },
        Error::CodeFileUnreadable,
        Error::ConnectionBusy,
        Error::ConnectionNotReady,
        Error::DeletionLookup,
        Error::DetachedHead,
        Error::EmptyTabName,
        Error::EmptyWorkspaceLabel,
        Error::GitCommonDir {
            path: path.clone(),
            source: io(),
        },
        Error::GitHubSecretInConfig("token"),
        Error::ImageAnimationResize,
        Error::ImageDecode(image()),
        Error::ImageDecodeLimit,
        Error::ImageEncode(image()),
        Error::ImageFileType,
        Error::ImageFormat,
        Error::ImageInputTooLarge { limit: 1024 },
        Error::ImageReadTimeout,
        Error::ImageSize,
        Error::ImageTooLarge { limit: 2048 },
        Error::InvalidAnnotation,
        Error::InvalidBranchName,
        Error::InvalidBrowserTabs,
        Error::InvalidBrowserUrl,
        Error::InvalidCodeFile,
        Error::InvalidExternalUrl,
        Error::InvalidGroupLayouts,
        Error::InvalidLocalPage,
        Error::InvalidReviewCheckout,
        Error::InvalidSidebarWidth,
        Error::InvalidStoredWidth,
        Error::InvalidWindowState,
        Error::InvalidWorktreeNotes,
        Error::LinkedFileMissing { path: path.clone() },
        Error::LinkOnAnotherHost {
            host: "build-box".into(),
        },
        Error::LocalAddresses(io()),
        Error::LocalGit(io()),
        Error::LocalNetworkDenied,
        Error::LocalWorktreeNote,
        Error::LogRead {
            path: path.clone(),
            source: io(),
        },
        Error::MalformedLink,
        Error::MissingDeletion,
        Error::MissingHome,
        Error::MissingStateRoot,
        Error::NoSnapshot,
        Error::NoTab,
        Error::NotAGitRepository { path: path.clone() },
        Error::NotConnected,
        Error::NotInCheckout,
        Error::PaneImageDecode(image()),
        Error::PaneImageLimit,
        Error::Path {
            path: path.clone(),
            source: Box::new(Error::NoTab),
        },
        Error::PreferencesPath,
        Error::RelativeConfigRoot,
        Error::ScreenshotImage,
        Error::ScreenshotWrite {
            path: path.clone(),
            source: io(),
        },
        Error::SelectionOffscreen,
        Error::SelectionSize,
        Error::SelectionStale,
        Error::SettingsSave(Arc::new(Error::NotConnected)),
        Error::SkillInstall {
            path: path.clone(),
            source: io(),
        },
        Error::SshConfig {
            path: path.clone(),
            source: io(),
        },
        Error::StaleCloseTarget,
        Error::StaleConnection,
        Error::StalePane,
        Error::StaleTab,
        Error::StaleWorkspace,
        Error::StateFileSize {
            path: path.clone(),
            limit: 4096,
        },
        Error::UnsupportedCommand,
        Error::WorkspaceCheckoutChanged,
        Error::WorkspaceGroupChanged,
        Error::WorkspaceRepositoryChanged,
        Error::WorktreeList,
        Error::WorktreeListDecode(json()),
        Error::WorktreeSelection,
    ];
    errors.extend(
        ["open", "inspect", "read"].map(|operation| Error::ImageFile {
            operation,
            source: io(),
        }),
    );
    #[cfg(feature = "azure")]
    errors.push(Error::CloudHostUnsupported);
    #[cfg(target_os = "macos")]
    errors.extend([Error::PageStack, Error::PopupOpener]);
    assert_localized(&errors);
}

/// A message holding another error shows that error's own message.
#[test]
fn wrapped_errors_show_their_own_message() {
    crate::i18n::show(&Pseudo::Accented.locale());
    let inner = Error::NoTab.localize();
    let outer = Error::Path {
        path: "notes.md".into(),
        source: Box::new(Error::NoTab),
    }
    .localize();
    crate::i18n::show(&Locale::FALLBACK);
    assert!(outer.contains(&inner), "{outer} lacks {inner}");
}

#[test]
fn system_and_parser_text_shows_as_it_is() {
    assert_verbatim(&[
        Error::Io(io()),
        Error::Json(json()),
        Error::Toml(toml::from_str::<toml::Table>("=").unwrap_err()),
        Error::TomlEdit("=".parse::<toml_edit::DocumentMut>().unwrap_err()),
    ]);
}
