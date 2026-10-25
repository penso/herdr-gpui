use super::*;
use crate::config::{
    preferences::Preference,
    remote::{Bind, Certificate, Edit, Privacy},
};

#[test]
fn remote_control_is_off_with_its_defaults() -> anyhow::Result<()> {
    let config = Config::parse("")?;
    assert_eq!(config.remote, RemoteConfig::default());
    assert!(!config.remote.enabled);
    assert!(config.remote.keep_running);
    assert_eq!(config.remote.port, 7443);
    assert_eq!(config.remote.bind, Bind::All);
    assert_eq!(config.remote.certificate, Certificate::Own);
    assert_eq!(config.remote.notifications, Privacy::Generic);
    assert_eq!(config.remote.device_idle_days, 30);
    // The managed file documents the table without setting it.
    assert_eq!(
        Config::parse(DEFAULT_CONFIG)?.remote,
        RemoteConfig::default()
    );
    Ok(())
}

/// An absolute path on this platform, for TOML literal strings: `/c.pem`
/// is relative on Windows, which needs a drive.
fn absolute(name: &str) -> PathBuf {
    env::temp_dir().join(name)
}

#[test]
fn remote_control_reads_every_key() -> anyhow::Result<()> {
    let (cert, key) = (absolute("cert.pem"), absolute("key.pem"));
    let config = Config::parse(&format!(
        "[remote]\nenabled = true\nport = 8443\nbind = '192.168.1.20'\n\
         certificate = {{ cert = '{}', key = '{}' }}\n\
         keep_running = false\nnotifications = 'excerpt'\ndevice_idle_days = 7\n",
        cert.display(),
        key.display(),
    ))?;
    assert!(config.unknown_keys.is_empty(), "{:?}", config.unknown_keys);
    let remote = config.remote;
    assert!(remote.enabled);
    assert!(!remote.keep_running);
    assert_eq!(remote.port, 8443);
    assert_eq!(remote.bind, Bind::Address("192.168.1.20".parse()?));
    assert_eq!(remote.bind.to_string(), "192.168.1.20");
    assert_eq!(remote.certificate, Certificate::Files { cert, key });
    assert_eq!(remote.notifications, Privacy::Excerpt);
    assert_eq!(remote.device_idle_days, 7);
    let ipv6 = Config::parse("[remote]\nbind = '::1'\ncertificate = 'own'")?;
    assert_eq!(ipv6.remote.bind, Bind::Address("::1".parse()?));
    assert_eq!(ipv6.remote.certificate, Certificate::Own);
    Ok(())
}

#[test]
fn remote_control_refuses_values_the_daemon_cannot_use() {
    let (cert, key) = (absolute("c.pem"), absolute("k.pem"));
    for (text, expected) in [
        ("port = 0".to_owned(), "port"),
        ("device_idle_days = 0".to_owned(), "idle"),
        ("device_idle_days = 366".to_owned(), "idle"),
        (
            format!(
                "certificate = {{ cert = 'cert.pem', key = '{}' }}",
                key.display()
            ),
            "cert",
        ),
        (
            format!(
                "certificate = {{ cert = '{}', key = 'key.pem' }}",
                cert.display()
            ),
            "key",
        ),
    ] {
        let Err(error) = Config::parse(&format!("[remote]\n{text}")) else {
            panic!("{text} loaded");
        };
        let matched = match (&error, expected) {
            (crate::Error::RemoteControl(crate::remote::Error::Port), "port") => true,
            (crate::Error::RemoteControl(crate::remote::Error::IdleDays), "idle") => true,
            (
                crate::Error::RemoteControl(crate::remote::Error::CertificatePath { which }),
                which_expected,
            ) => *which == which_expected,
            _ => false,
        };
        assert!(matched, "{text}: {error:?}");
    }
    for text in [
        "port = 70000",
        "port = -1",
        "bind = 'everywhere'",
        "bind = 7",
        "certificate = 'theirs'",
        "certificate = { cert = '/c.pem' }",
        "certificate = { cert = '/c.pem', key = '/k.pem', chain = '/x.pem' }",
        "notifications = 'full'",
        "enabled = 'yes'",
    ] {
        assert!(
            Config::parse(&format!("[remote]\n{text}")).is_err(),
            "{text} loaded"
        );
    }
}

#[test]
fn unknown_remote_keys_are_reported_not_fatal() -> anyhow::Result<()> {
    let config = Config::parse("[remote]\nenabled = true\nrelay = 'x'")?;
    assert!(config.remote.enabled);
    assert_eq!(config.unknown_keys, ["remote.relay"]);
    Ok(())
}

#[test]
fn remote_edits_write_what_loading_reads() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    fs::write(&path, "# mine\n")?;
    let files = Certificate::Files {
        cert: env::temp_dir().join("cert.pem"),
        key: env::temp_dir().join("key.pem"),
    };
    for edit in [
        Edit::Enabled(true),
        Edit::Port(9443),
        Edit::Bind(Bind::Address("10.0.0.2".parse()?)),
        Edit::Certificate(files.clone()),
        Edit::KeepRunning(false),
        Edit::Notifications(Privacy::Excerpt),
        Edit::DeviceIdleDays(90),
    ] {
        Config::save_preference_path(Preference::Remote(edit), &path)?;
    }
    let remote = Config::parse(&fs::read_to_string(&path)?)?.remote;
    assert_eq!(
        remote,
        RemoteConfig {
            bind: Bind::Address("10.0.0.2".parse()?),
            certificate: files,
            device_idle_days: 90,
            enabled: true,
            keep_running: false,
            notifications: Privacy::Excerpt,
            port: 9443,
        }
    );
    Config::save_preference_path(
        Preference::Remote(Edit::Certificate(Certificate::Own)),
        &path,
    )?;
    Config::save_preference_path(Preference::Remote(Edit::Bind(Bind::All)), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# mine"), "{text}");
    let remote = Config::parse(&text)?.remote;
    assert_eq!(remote.certificate, Certificate::Own);
    assert_eq!(remote.bind, Bind::All);
    Ok(())
}

#[test]
fn invalid_remote_edits_leave_the_file_alone() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    fs::write(&path, "[remote]\nport = 8000\n")?;
    for edit in [
        Edit::Port(0),
        Edit::DeviceIdleDays(0),
        Edit::Certificate(Certificate::Files {
            cert: "relative.pem".into(),
            key: absolute("key.pem"),
        }),
    ] {
        assert!(Config::save_preference_path(Preference::Remote(edit), &path).is_err());
    }
    assert_eq!(fs::read_to_string(&path)?, "[remote]\nport = 8000\n");
    Ok(())
}
