//! The `[container]` table: how new containers on this computer's Docker or
//! Podman engine are made. A build without the `container` feature still
//! parses the table, so one config file serves every build, but never reads
//! it. Numbers may be written as TOML numbers or strings (`cpus = 2` or
//! `cpus = "1.5"`); Settings writes strings.
#[cfg(feature = "container")]
use crate::Result;
use serde::{Deserialize, Deserializer};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContainerConfig {
    // keep-sorted start
    /// Stop a running container once none of its agents has worked or
    /// waited for input for this long, such as `30m` or `2h`; `never` by
    /// default. Its files stay, and the next connection starts it again.
    pub auto_stop: Option<String>,
    #[serde(deserialize_with = "text")]
    pub cpus: Option<String>,
    /// `docker` or `podman`; the first one installed when unset.
    pub engine: Option<String>,
    /// The image new containers start from, such as `ubuntu:24.04`.
    pub image: Option<String>,
    /// The most containers kept as devices at once; 8 by default.
    #[serde(deserialize_with = "text")]
    pub max_containers: Option<String>,
    #[serde(deserialize_with = "text")]
    pub memory: Option<String>,
    #[serde(deserialize_with = "text")]
    pub pids_limit: Option<String>,
    // keep-sorted end
}

/// A string, or a number read as its text.
fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Text {
        Integer(i64),
        Float(f64),
        String(String),
    }
    Ok(Some(match Text::deserialize(deserializer)? {
        Text::Integer(value) => value.to_string(),
        Text::Float(value) => value.to_string(),
        Text::String(value) => value,
    }))
}

#[cfg(feature = "container")]
impl ContainerConfig {
    /// The validated settings new containers are made with.
    pub fn settings(&self) -> Result<crate::container::Settings> {
        Ok(crate::container::Settings::resolve(self)?)
    }
}

/// The `[container]` keys the Settings window edits. An empty value removes
/// its key.
#[cfg(feature = "container")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContainerFields {
    pub engine: String,
    pub image: String,
    pub cpus: String,
    pub memory: String,
    pub auto_stop: String,
}

#[cfg(feature = "container")]
impl ContainerFields {
    pub fn from_config(config: &ContainerConfig) -> Self {
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        Self {
            engine: text(&config.engine),
            image: text(&config.image),
            cpus: text(&config.cpus),
            memory: text(&config.memory),
            auto_stop: text(&config.auto_stop),
        }
    }

    pub fn entries(&self) -> [(&'static str, &str); 5] {
        [
            ("engine", &self.engine),
            ("image", &self.image),
            ("cpus", &self.cpus),
            ("memory", &self.memory),
            ("auto_stop", &self.auto_stop),
        ]
    }

    /// The checks loading applies, so a save cannot leave a config that then
    /// fails to load.
    pub fn validate(&self, existing: &ContainerConfig) -> Result<()> {
        let value = |text: &str| {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        };
        ContainerConfig {
            engine: value(&self.engine),
            image: value(&self.image),
            cpus: value(&self.cpus),
            memory: value(&self.memory),
            auto_stop: value(&self.auto_stop),
            ..existing.clone()
        }
        .settings()
        .map(drop)
    }
}
