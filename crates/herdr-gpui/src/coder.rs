//! Coder workspaces as Herdr devices: OAuth2 sign-in against a configured
//! deployment and bounded REST calls. Everything here blocks, so it runs on
//! background workers and reports back through the window's mailboxes.

mod api;
mod catalog;
mod connect;
mod error;
mod http;
mod install;
#[cfg(test)]
mod mock;
mod names;
mod oauth;
mod settings;
pub(crate) mod setup;
mod store;
mod token;

pub use error::{Error, Status};
pub(crate) use {
    api::{Preset, Progress, Template, Workspace},
    catalog::{SavedWorkspace, load as load_workspaces},
    connect::connect,
    names::{suggest as suggest_name, valid as valid_name},
    settings::Settings,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
pub(crate) mod tests {
    use super::{settings::Redirect, *};

    /// A valid deployment for unit tests; nothing here is ever contacted.
    pub(crate) fn settings() -> Settings {
        Settings {
            base: "https://coder.example.com".into(),
            client_id: "client-fixture".into(),
            client_secret: "secret-fixture".into(),
            redirect: Redirect {
                uri: "http://127.0.0.1:47823/callback".into(),
                path: "/callback".into(),
                address: std::net::SocketAddr::from(([127, 0, 0, 1], 47823)),
            },
            organization: None,
            workspace_prefix: "herdr".into(),
            cli: None,
            store: crate::github::Store::Environment,
        }
    }
}
