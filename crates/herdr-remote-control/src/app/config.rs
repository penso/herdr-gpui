//! The `[remote]` table: remote control from a phone, served by the
//! `herdr-remote-control` daemon that Herdr UI installs as a user service.
//!
//! ```toml
//! [remote]
//! enabled = true
//! port = 7443
//! bind = "all"                 # or one interface address, "192.168.1.20"
//! certificate = "own"          # or { cert = "/path/cert.pem", key = "/path/key.pem" }
//! keep_running = true          # the daemon outlives Herdr UI
//! notifications = "generic"    # or "excerpt": a short excerpt in each push
//! device_idle_days = 30         # 0 never forgets a device
//! ```
//!
//! Secrets and device keys never go here: the daemon keeps them in its own
//! private state directory.

use crate::app::Error;
use serde::Deserialize;
use std::{fmt, net::IpAddr, ops::RangeInclusive, path::PathBuf};

pub const DEFAULT_PORT: u16 = 7443;
const DEFAULT_IDLE_DAYS: u16 = 30;
/// How long a paired device may stay away before the daemon forgets it;
/// 0 keeps it until it is revoked.
pub const IDLE_DAYS: RangeInclusive<u16> = 0..=365;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RemoteConfig {
    // keep-sorted start
    /// Where HTTPS listens: every interface, or one address.
    pub bind: Bind,
    pub certificate: Certificate,
    pub device_idle_days: u16,
    /// Whether the daemon is installed as a user service and runs.
    pub enabled: bool,
    /// Whether the daemon keeps serving phones after Herdr UI quits.
    pub keep_running: bool,
    pub notifications: Privacy,
    pub port: u16,
    // keep-sorted end
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            // keep-sorted start
            bind: Bind::default(),
            certificate: Certificate::default(),
            device_idle_days: DEFAULT_IDLE_DAYS,
            enabled: false,
            keep_running: true,
            notifications: Privacy::default(),
            port: DEFAULT_PORT,
            // keep-sorted end
        }
    }
}

impl RemoteConfig {
    /// Refuses values the daemon could not start with.
    pub fn check(&self) -> Result<(), Error> {
        if self.port == 0 {
            return Err(Error::Port);
        }
        if !IDLE_DAYS.contains(&self.device_idle_days) {
            return Err(Error::IdleDays);
        }
        if let Certificate::Files { cert, key } = &self.certificate {
            for (which, path) in [("cert", cert), ("key", key)] {
                if !path.is_absolute() {
                    return Err(Error::CertificatePath { which });
                }
            }
        }
        Ok(())
    }
}

/// Where the daemon's HTTPS server listens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Bind {
    /// Every interface: the LAN, and Tailscale when it runs.
    #[default]
    All,
    /// One interface's address.
    Address(IpAddr),
}

impl fmt::Display for Bind {
    /// As the config file and the daemon's `--bind` write it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => f.write_str("all"),
            Self::Address(address) => address.fmt(f),
        }
    }
}

impl<'de> Deserialize<'de> for Bind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == "all" {
            return Ok(Self::All);
        }
        text.parse().map(Self::Address).map_err(|_| {
            serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&text),
                &"\"all\" or an IP address",
            )
        })
    }
}

/// The certificate HTTPS presents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Certificate {
    /// The daemon's own certificate authority, trusted once on the phone.
    #[default]
    Own,
    /// A certificate and key the user has, such as for their own domain.
    Files { cert: PathBuf, key: PathBuf },
}

impl<'de> Deserialize<'de> for Certificate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Files {
            cert: PathBuf,
            key: PathBuf,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Name(String),
            Files(Files),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Name(name) if name == "own" => Ok(Self::Own),
            Raw::Name(name) => Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&name),
                &"\"own\" or { cert = PATH, key = PATH }",
            )),
            Raw::Files(Files { cert, key }) => Ok(Self::Files { cert, key }),
        }
    }
}

/// What a push notification says about the chat it is for.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Privacy {
    /// "Claude needs an answer in herdr-gpui · main", without message text.
    #[default]
    Generic,
    /// Also a short excerpt of the message.
    Excerpt,
}

impl Privacy {
    /// As the config file and the daemon's `--privacy` write it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Excerpt => "excerpt",
        }
    }
}

/// One change to the `[remote]` table, from Settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Bind(Bind),
    Certificate(Certificate),
    DeviceIdleDays(u16),
    Enabled(bool),
    KeepRunning(bool),
    Notifications(Privacy),
    Port(u16),
}

impl Edit {
    /// The key and value the edit writes, or why it may not.
    pub fn entry(self) -> Result<(&'static str, toml_edit::Value), Error> {
        Ok(match self {
            Self::Bind(bind) => ("bind", bind.to_string().into()),
            Self::Certificate(Certificate::Own) => ("certificate", "own".into()),
            Self::Certificate(Certificate::Files { cert, key }) => {
                let text = |which, path: PathBuf| {
                    path.is_absolute()
                        .then(|| path.to_str().map(str::to_owned))
                        .flatten()
                        .ok_or(Error::CertificatePath { which })
                };
                let mut table = toml_edit::InlineTable::new();
                table.insert("cert", text("cert", cert)?.into());
                table.insert("key", text("key", key)?.into());
                ("certificate", toml_edit::Value::InlineTable(table))
            }
            Self::DeviceIdleDays(days) => {
                if !IDLE_DAYS.contains(&days) {
                    return Err(Error::IdleDays);
                }
                ("device_idle_days", i64::from(days).into())
            }
            Self::Enabled(enabled) => ("enabled", enabled.into()),
            Self::KeepRunning(keep) => ("keep_running", keep.into()),
            Self::Notifications(privacy) => ("notifications", privacy.name().into()),
            Self::Port(0) => return Err(Error::Port),
            Self::Port(port) => ("port", i64::from(port).into()),
        })
    }
}
