#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid saved notification mutes")]
    Invalid,
    #[error("Cannot read saved notification mutes: {0}")]
    Json(#[from] serde_json::Error),
}
