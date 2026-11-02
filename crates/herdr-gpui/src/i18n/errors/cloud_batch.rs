//! Cloud machines an agent asked for: `error-cloud-batch.ftl`.

use super::Localize;
use crate::{cloud::batch::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // `herdr_remote::cloud::exec::Error` has no mapping yet, so it
            // shows its English `Display`.
            Self::Exec(error) => error.to_string(),
            Self::NotOffered(provider) => {
                fl!("error-cloud-batch-not-offered", provider = *provider)
            }
            // The script's output, as it printed it.
            Self::Setup { code, output } => match (code, output.is_empty()) {
                (Some(code), true) => fl!("error-cloud-batch-setup-exited", code = *code),
                (Some(code), false) => fl!(
                    "error-cloud-batch-setup-exited-output",
                    code = *code,
                    output = output.as_str()
                ),
                (None, true) => fl!("error-cloud-batch-setup-signal"),
                (None, false) => {
                    fl!(
                        "error-cloud-batch-setup-signal-output",
                        output = output.as_str()
                    )
                }
            },
        }
    }
}
