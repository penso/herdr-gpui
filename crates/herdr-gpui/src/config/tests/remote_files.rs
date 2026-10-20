use super::*;

#[test]
fn remote_folder_sizes_default_and_overlay_like_notifications() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
    ] {
        assert_eq!(
            config.remote_files,
            RemoteFilesConfig {
                folder_warning_mb: 64,
                folder_transfer_cap_mb: 512,
            }
        );
    }
    let config =
        Config::parse("[remote_files]\nfolder_warning_mb = 100\nfolder_transfer_cap_mb = 200")?;
    assert_eq!(
        config.remote_files,
        RemoteFilesConfig {
            folder_warning_mb: 100,
            folder_transfer_cap_mb: 200,
        }
    );
    // Naming one key leaves the other at its default.
    let config = Config::parse("[remote_files]\nfolder_warning_mb = 100")?;
    assert_eq!(config.remote_files.folder_warning_mb, 100);
    assert_eq!(config.remote_files.folder_transfer_cap_mb, 512);
    Ok(())
}

#[test]
fn remote_folder_sizes_accept_their_bounds_and_refuse_one_past() -> anyhow::Result<()> {
    for text in [
        "[remote_files]\nfolder_warning_mb = 1",
        "[remote_files]\nfolder_warning_mb = 4096\nfolder_transfer_cap_mb = 8192",
        "[remote_files]\nfolder_warning_mb = 1\nfolder_transfer_cap_mb = 1",
        "[remote_files]\nfolder_transfer_cap_mb = 8192",
        // The two may meet; the warning fires at the cap.
        "[remote_files]\nfolder_warning_mb = 512",
    ] {
        assert!(Config::parse(text).is_ok(), "rejected {text:?}");
    }
    for text in [
        "[remote_files]\nfolder_warning_mb = 0",
        "[remote_files]\nfolder_warning_mb = 4097",
        "[remote_files]\nfolder_transfer_cap_mb = 0",
        "[remote_files]\nfolder_transfer_cap_mb = 8193",
    ] {
        assert!(Config::parse(text).is_err(), "accepted {text:?}");
    }
    Ok(())
}

#[test]
fn remote_folder_size_range_messages_name_their_key() -> anyhow::Result<()> {
    for (text, expected) in [
        (
            "[remote_files]\nfolder_warning_mb = 0",
            "remote_files.folder_warning_mb must be between 1 and 4096",
        ),
        (
            "[remote_files]\nfolder_transfer_cap_mb = 0",
            "remote_files.folder_transfer_cap_mb must be between 1 and 8192",
        ),
    ] {
        let error = Config::parse(text)
            .err()
            .ok_or_else(|| anyhow::anyhow!("accepted {text:?}"))?;
        assert!(error.to_string().contains(expected), "{error}");
    }
    Ok(())
}

#[test]
fn a_warning_above_the_cap_is_refused_at_load() -> anyhow::Result<()> {
    let text = "[remote_files]\nfolder_warning_mb = 1024\nfolder_transfer_cap_mb = 512";
    let error = Config::parse(text)
        .err()
        .ok_or_else(|| anyhow::anyhow!("accepted a warning above the cap"))?;
    assert!(matches!(error, Error::Toml(_)), "{error:?}");
    assert!(std::error::Error::source(&error).is_some());
    assert!(
        error.to_string().contains(
            "remote_files.folder_warning_mb must not exceed remote_files.folder_transfer_cap_mb"
        ),
        "{error}"
    );
    Ok(())
}

#[test]
fn unknown_remote_files_keys_are_reported() -> anyhow::Result<()> {
    assert_eq!(
        Config::parse("[remote_files]\nfuture = 1")?.unknown_keys,
        ["remote_files.future"]
    );
    Ok(())
}
