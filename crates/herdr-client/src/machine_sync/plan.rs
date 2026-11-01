//! What one sync changes, worked out from what every machine reported before
//! anything is written. Pure, so every rule is tested without SSH.
//!
//! Catalogs only grow: a profile missing from one is appended, and nothing a
//! machine already lists is changed or removed. A profile is the same one in
//! two catalogs when they share its ID, or its exact target and session: one
//! machine may be saved once per Herdr session on it.
use super::{SyncState, keys::PublicKey, remote::Collected};
use crate::{
    SavedHost,
    catalog::{Catalog, MAX_HOSTS, encode_catalog},
};

/// A saved device of this machine that takes part, and what it reported, or
/// `None` when it could not be read.
pub(crate) struct Member<'a> {
    pub(crate) host: &'a SavedHost,
    pub(crate) collected: Option<&'a Collected>,
}

/// One saved profile of the list, under every ID it is known by.
struct Profile {
    /// The ID other catalogs get first, then the machine's own.
    ids: Vec<String>,
    label: String,
    target: String,
    session: String,
}

impl Profile {
    fn of(host: &SavedHost) -> Self {
        Self {
            ids: vec![host.id.clone()],
            label: host.label.clone(),
            target: host.target.clone(),
            session: host.session.clone(),
        }
    }

    fn matches(&self, host: &SavedHost) -> bool {
        self.ids.contains(&host.id) || same_profile(&self.target, &self.session, host)
    }

    /// Whether `host` is this profile's machine, under any session. Leaving a
    /// machine out leaves out every profile on it.
    fn same_machine(&self, host: &SavedHost) -> bool {
        self.ids.contains(&host.id) || self.target == host.target
    }

    fn host(&self) -> SavedHost {
        SavedHost {
            id: self.ids[0].clone(),
            label: self.label.clone(),
            target: self.target.clone(),
            session: self.session.clone(),
            enabled: true,
        }
    }
}

/// Whether `host` is saved for exactly `target` and `session`.
fn same_profile(target: &str, session: &str, host: &SavedHost) -> bool {
    host.target == target && host.session == session
}

/// Appends `host` unless the catalog would pass its byte limit, which a
/// catalog of long targets can reach before [`MAX_HOSTS`]. Returns whether
/// it was added.
fn push_within_limit(catalog: &mut Catalog, host: SavedHost) -> super::Result<bool> {
    catalog.ssh.push(host);
    match encode_catalog(catalog) {
        Ok(_) => Ok(true),
        Err(crate::Error::CatalogLimit) => {
            catalog.ssh.pop();
            Ok(false)
        }
        Err(error) => Err(super::Error::Catalog(Box::new(error))),
    }
}

/// What to write on one member.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct MemberWrite {
    /// `authorized_keys` lines of every other machine's key.
    pub(crate) keys: Vec<String>,
    /// The catalog with the profiles it lacked, or `None` when it lacks none.
    pub(crate) catalog: Option<Vec<u8>>,
    /// How many profiles `catalog` adds.
    pub(crate) devices: usize,
    /// The sync state to give the member when it has none, so it keeps the
    /// list in step from then on too.
    pub(crate) state: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Plan {
    /// Machines another member lists that this one does not, to save here.
    pub(crate) learned: Vec<SavedHost>,
    /// Each member's write, in member order; `None` for one not read.
    pub(crate) writes: Vec<Option<MemberWrite>>,
    /// Every member's key, with its label, to authorize here.
    pub(crate) local_keys: Vec<(PublicKey, String)>,
}

/// The plan for this machine (`state`, holding `own_key`), whose catalog is
/// `local`, and its `members`.
pub(crate) fn plan(
    state: &SyncState,
    own_key: &PublicKey,
    local: &[SavedHost],
    members: &[Member],
) -> super::Result<Plan> {
    // This machine first, then one machine per member, in member order.
    let mut profiles = vec![Profile {
        ids: vec![state.id.clone()],
        label: state.label.clone(),
        target: state.target.clone(),
        session: "default".to_owned(),
    }];
    for member in members {
        let mut profile = Profile::of(member.host);
        if let Some(own) = member.collected.and_then(|c| c.state.as_ref())
            && own.id != member.host.id
        {
            profile.ids.push(own.id.clone());
        }
        profiles.push(profile);
    }
    let mut learned = Vec::new();
    for collected in members.iter().filter_map(|member| member.collected) {
        let excluded = |host: &SavedHost| {
            collected
                .state
                .as_ref()
                .is_some_and(|state| state.excluded.contains(&host.id))
        };
        for host in &collected.catalog.ssh {
            let known = local.iter().any(|saved| {
                saved.id == host.id || same_profile(&saved.target, &saved.session, host)
            }) || profiles.iter().any(|profile| profile.matches(host));
            if !host.enabled || excluded(host) || known {
                continue;
            }
            learned.push(host.clone());
            profiles.push(Profile::of(host));
        }
    }
    let mut writes = Vec::with_capacity(members.len());
    for (index, member) in members.iter().enumerate() {
        let Some(collected) = member.collected else {
            writes.push(None);
            continue;
        };
        // The member's own machine is never listed or authorized on itself,
        // under any session.
        let itself = |position: usize, profile: &Profile| {
            position == index + 1 || profile.target == member.host.target
        };
        // The member may know a machine under its own ID, matched by target,
        // and leave it out under that ID.
        let excluded =
            |profile: &Profile| {
                collected.state.as_ref().is_some_and(|state| {
                    profile
                        .ids
                        .iter()
                        .any(|id| state.excluded.contains(id.as_str()))
                        || collected.catalog.ssh.iter().any(|host| {
                            profile.same_machine(host) && state.excluded.contains(&host.id)
                        })
                })
            };
        let mut catalog = collected.catalog.clone();
        let mut devices = 0;
        for (position, profile) in profiles.iter().enumerate() {
            if catalog.ssh.len() >= MAX_HOSTS {
                break;
            }
            if itself(position, profile)
                || excluded(profile)
                || catalog.ssh.iter().any(|host| profile.matches(host))
            {
                continue;
            }
            if !push_within_limit(&mut catalog, profile.host())? {
                break;
            }
            devices += 1;
        }
        let mut keys = Vec::new();
        if !excluded(&profiles[0]) {
            keys.push(own_key.line(&state.label));
        }
        for (other, member) in members.iter().enumerate() {
            let Some(key) = member.collected.map(|c| &c.key) else {
                continue;
            };
            if other == index || excluded(&profiles[other + 1]) || key == &collected.key {
                continue;
            }
            keys.push(key.line(&member.host.label));
        }
        let state = match &collected.state {
            Some(_) => None,
            None => Some(super::encode_state(&SyncState {
                id: member.host.id.clone(),
                label: member.host.label.clone(),
                target: member.host.target.clone(),
                excluded: Default::default(),
            })?),
        };
        writes.push(Some(MemberWrite {
            keys,
            catalog: (devices > 0)
                .then(|| encode_catalog(&catalog))
                .transpose()
                .map_err(|error| super::Error::Catalog(Box::new(error)))?,
            devices,
            state,
        }));
    }
    let local_keys = members
        .iter()
        .filter_map(|member| Some((member.collected?.key.clone(), member.host.label.clone())))
        .collect();
    Ok(Plan {
        learned,
        writes,
        local_keys,
    })
}
