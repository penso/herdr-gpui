//! Coder, the first `cloud` provider: OAuth2 sign-in against a configured
//! deployment, bounded REST calls, workspace readiness, and the `coder ssh`
//! command that reaches a workspace. Everything here blocks, so it runs on
//! background workers and reports back through the window's mailboxes.

mod api;
mod connect;
pub mod create;
mod destroy;
mod error;
mod http;
mod names;
mod oauth;
mod settings;
pub mod setup;
mod store;
mod token;

pub use error::{Error, Status};
pub use {
    api::{Preset, Template, Workspace},
    connect::{command, connect},
    names::{random as random_name, valid as valid_name},
    settings::Settings,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A Coder deployment for tests here and in the app; nothing is ever contacted.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::{Settings, settings::Redirect};

    /// A valid deployment; nothing here is ever contacted.
    pub fn settings() -> Settings {
        Settings {
            base: "https://coder.example.com".into(),
            client_id: "client-fixture".into(),
            client_secret: Some("secret-fixture".into()),
            redirect: Redirect {
                uri: "http://127.0.0.1:47823/callback".into(),
                path: "/callback".into(),
                address: std::net::SocketAddr::from(([127, 0, 0, 1], 47823)),
            },
            organization: None,
            workspace_prefix: "herdr".into(),
            cli: None,
            store: crate::credentials::Store::Environment,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
