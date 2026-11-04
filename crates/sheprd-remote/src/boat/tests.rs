use super::*;
pub use super::{api::wait_ready, http::Request};

pub mod mock;

pub fn settings() -> Settings {
    Settings {
        base: "https://boat.example.com/api/v1".into(),
        organization: None,
        machine_type: None,
        environment: None,
        ttl_seconds: None,
        store: crate::credentials::Store::Environment,
    }
}

impl Request<'_> {
    pub fn create_for_test(
        &self,
        name: &str,
        cancelled: impl Fn() -> bool,
        remember: impl FnOnce(&api::Sandbox) -> Result<()>,
    ) -> Result<api::Sandbox> {
        let creation = pending::Creation::new(self.settings, name, 0);
        self.create(&creation, cancelled, remember)
    }
}
