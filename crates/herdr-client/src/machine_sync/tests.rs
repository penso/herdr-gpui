use super::*;
use crate::catalog::Catalog;

mod key_lines;
mod planning;
mod replies;
mod state_file;

fn id(n: u8) -> String {
    format!("{n:032x}")
}

fn host(n: u8, target: &str) -> SavedHost {
    SavedHost {
        id: id(n),
        label: format!("machine-{n}"),
        target: target.to_owned(),
        session: "default".to_owned(),
        enabled: true,
    }
}

fn key(n: u8) -> keys::PublicKey {
    keys::PublicKey::parse(&format!("ssh-ed25519 AAAAC3Nza{n:02} comment")).unwrap()
}

fn state(n: u8, target: &str) -> SyncState {
    SyncState::new(id(n), format!("machine-{n}"), target.to_owned()).unwrap()
}

fn collected(n: u8, hosts: Vec<SavedHost>, state: Option<SyncState>) -> remote::Collected {
    remote::Collected {
        key: key(n),
        sum: "1 2".to_owned(),
        catalog: Catalog {
            version: 1,
            selected_profile: None,
            ssh: hosts,
        },
        state,
    }
}

/// The hosts a planned catalog write lists.
fn written(write: &plan::MemberWrite) -> Vec<SavedHost> {
    write
        .catalog
        .as_deref()
        .map(|bytes| catalog::parse_catalog(bytes).unwrap().ssh)
        .unwrap_or_default()
}
