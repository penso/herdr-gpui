//! boxd failures keep their category and cause until the UI shows them.
//! Remote text is a bounded diagnostic payload, never a credential.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "{0} must be an https origin (http only on loopback) with no credentials, path, query, or fragment."
    )]
    Url(&'static str),
    #[error("{0} is not valid.")]
    Field(crate::cloud::Value),
    #[error("{0} must be UTF-8.")]
    Encoding(crate::cloud::Value),
    #[error("boxd needs an API key. Save one in Settings > Cloud devices.")]
    MissingKey,
    #[error("boxd rejected the API key.")]
    Authentication,
    #[error("boxd denied access. Check the API key's organization.")]
    Forbidden,
    #[error("The boxd machine {0} no longer exists.")]
    Gone(String),
    #[error("The boxd machine {0} failed and cannot start.")]
    Failed(String),
    #[error("The boxd machine {0} did not start in time.")]
    NotReady(String),
    #[error("{0}")]
    Status(#[from] Status),
    #[error("boxd network request failed or timed out.")]
    Network(#[source] ureq::Error),
    #[error("boxd response could not be read within the size/time limit.")]
    Read(#[source] io::Error),
    #[error("boxd response exceeded the size limit.")]
    Size,
    #[error("Invalid boxd API response.")]
    Wire,
    #[error("Invalid boxd authorization header.")]
    Header(#[source] ureq::http::header::InvalidHeaderValue),
    #[error(transparent)]
    Socket(#[from] crate::cloud::socket::Error),
    #[error("The boxd exec stream failed.")]
    Stream(#[source] io::Error),
    #[error("Could not write the command's output.")]
    Output(#[source] io::Error),
    #[error("Could not find this app's executable to relay through.")]
    Executable(#[source] io::Error),
    #[error("Cancelled.")]
    Cancelled,
    #[error(
        "This device belongs to a boxd account that is no longer configured in [boxd] api_url."
    )]
    Account,
    #[error("{0}")]
    Bridge(#[source] sheprd_client::Error),
    #[error(transparent)]
    Cloud(#[from] crate::cloud::Error),
    #[error("{0}")]
    Storage(#[source] Box<crate::Error>),
}

/// A call boxd refused: its gRPC status code (or, with no gRPC answer at
/// all, the HTTP status) and its bounded, control-free message.
#[derive(Debug, thiserror::Error)]
#[error("boxd request failed ({}){}", self.code_text(), message.as_deref().map(|m| format!(": {m}")).unwrap_or_default())]
pub struct Status {
    pub code: Code,
    pub message: Option<String>,
}

/// Which kind of status refused a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    /// A gRPC status code, such as 5 for `NOT_FOUND`.
    Grpc(u32),
    /// An HTTP status that carried no gRPC answer, such as a proxy's 502.
    Http(u16),
}

impl Status {
    fn code_text(&self) -> String {
        match self.code {
            Code::Grpc(code) => format!("gRPC status {code}"),
            Code::Http(code) => format!("HTTP {code}"),
        }
    }
}
