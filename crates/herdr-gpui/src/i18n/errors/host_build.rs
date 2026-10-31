//! Copying a helper program onto a host: `error-host-build.ftl`.

use super::Localize;
use crate::{host_build::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Binary { program, path, .. } => fl!(
                "error-host-build-binary",
                program = *program,
                path = path.display().to_string()
            ),
            Self::Checksum => fl!("error-host-build-checksum"),
            Self::Platform(output) => {
                fl!("error-host-build-platform", output = output.as_str())
            } // keep-sorted end
        }
    }
}
