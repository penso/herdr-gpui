//! `herdr_client::machine_sync::Error`: syncing the saved machines between
//! the user's computers, from `error-client-machine-sync.ftl`.

use super::Localize;
use crate::i18n::fl;
use herdr_client::machine_sync::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::AuthorizedKeys { .. } => fl!("error-client-machine-sync-authorized-keys"),
            Self::Catalog(_) => fl!("error-client-machine-sync-catalog"),
            Self::Changed => fl!("error-client-machine-sync-changed"),
            Self::Home => fl!("error-client-machine-sync-home"),
            Self::KeyRead(_) => fl!("error-client-machine-sync-key-read"),
            Self::Keygen(status) => fl!(
                "error-client-machine-sync-keygen",
                status = status.to_string()
            ),
            Self::KeygenSpawn(_) => fl!("error-client-machine-sync-keygen-spawn"),
            Self::PublicKey => fl!("error-client-machine-sync-public-key"),
            Self::Reply => fl!("error-client-machine-sync-reply"),
            // The script's own failure says why the machine could not be reached.
            Self::Script(error) => error.localize(),
            Self::StateSchema(_) => fl!("error-client-machine-sync-state-schema"),
            Self::StateValue => fl!("error-client-machine-sync-state-value"),
            // keep-sorted end
        }
    }
}
