//! The handshake: which protocol generation and methods a core speaks.

use crate::Method;
use serde::Deserialize;

/// The protocol generation this client speaks.
pub const GENERATION: u32 = 1;
/// The oldest core this client was tested against.
pub const MIN_VERSION: &str = "0.1.13";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Hello {
    pub core: CoreInfo,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub framed: bool,
    pub generation: u32,
    #[serde(default)]
    pub methods: Vec<String>,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CoreInfo {
    /// Permanent random ID, reset only when the core's state is wiped.
    pub id: String,
    /// Display name, the hostname by default.
    pub name: String,
}

/// Why this client cannot use a core.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Mismatch {
    #[error("herdr-core lacks {}", names(.0))]
    Missing(Vec<Method>),
    #[error("herdr-core speaks protocol generation {0}; update Herdr UI")]
    Newer(u32),
    #[error("herdr-core {have} is older than {MIN_VERSION}; run `hc update`")]
    TooOld { have: String },
}

fn names(methods: &[Method]) -> String {
    methods
        .iter()
        .map(|method| method.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

impl Hello {
    pub fn check(&self) -> Result<(), Mismatch> {
        if self.generation > GENERATION {
            return Err(Mismatch::Newer(self.generation));
        }
        if self.generation < GENERATION || version(&self.version) < version(MIN_VERSION) {
            return Err(Mismatch::TooOld {
                have: self.version.clone(),
            });
        }
        let missing: Vec<Method> = Method::ALL
            .iter()
            .copied()
            .filter(|method| {
                method.required() && !self.methods.iter().any(|name| name == method.as_str())
            })
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Mismatch::Missing(missing))
        }
    }
}

/// `major.minor.patch`, ignoring a leading `hc ` and any suffix; missing or
/// unreadable parts count as zero.
fn version(text: &str) -> [u64; 3] {
    let mut parts = text
        .trim()
        .trim_start_matches("hc ")
        .split(['.', '-', '+'])
        .map(|part| part.parse::<u64>().unwrap_or(0));
    [(); 3].map(|()| parts.next().unwrap_or(0))
}
