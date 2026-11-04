use super::*;

mod mock;

/// Valid account settings for unit tests; nothing here is ever contacted.
pub fn settings() -> Settings {
    Settings {
        base: "https://daytona.example.com/api".into(),
        organization: None,
        target: None,
        snapshot: None,
        store: crate::credentials::Store::Environment,
    }
}
