//! What one sync changes, worked out from what every machine reported before
//! anything is written. Pure, so every rule is tested without SSH.
//!
//! Catalogs only grow: a machine missing from one is appended, and nothing a
//! machine already lists is changed or removed. A machine is the same one in
//! two catalogs when they share its profile ID or its exact target.
use super::{SyncState, keys::PublicKey, remote::Collected};
use crate::{
    SavedHost,
    catalog::{MAX_HOSTS, encode_catalog},
};

/// A saved device of this machine that takes part, and what it reported, or
/// `None` when it could not be read.
pub(crate) struct Member<'a> {
    pub(crate) host: &'a SavedHost,
    pub(crate) collected: Option<&'a Collected>,
}

/// One machine of the list, under every profile ID it is known by.
struct Machine {
    /// The ID other catalogs get first, then the machine's own.
    ids: Vec<String>,
    label: String,
    target: String,
    session: String,
}

impl Machine {
    fn of(host: &SavedHost) -> Self {
        Self {
            ids: vec![host.id.clone()],
            label: host.label.clone(),
            target: host.target.clone(),
            session: host.session.clone(),
        }
    }

    fn matches(&self, host: &SavedHost) -> bool {
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

/// What to write on one member.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct MemberWrite {
    /// `authorized_keys` lines of every other machine's key.
    pub(crate) keys: Vec<String>,
    /// The catalog with the machines it lacked, or `None` when it lacks none.
    pub(crate) catalog: Option<Vec<u8>>,
    /// How many machines `catalog` adds.
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
    let mut machines = vec![Machine {
        ids: vec![state.id.clone()],
        label: state.label.clone(),
        target: state.target.clone(),
        session: "default".to_owned(),
    }];
    for member in members {
        let mut machine = Machine::of(member.host);
        if let Some(own) = member.collected.and_then(|c| c.state.as_ref())
            && own.id != member.host.id
        {
            machine.ids.push(own.id.clone());
        }
        machines.push(machine);
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
            let known = local
                .iter()
                .any(|saved| saved.id == host.id || saved.target == host.target)
                || machines.iter().any(|machine| machine.matches(host));
            if !host.enabled || excluded(host) || known {
                continue;
            }
            learned.push(host.clone());
            machines.push(Machine::of(host));
        }
    }
    let mut writes = Vec::with_capacity(members.len());
    for (index, member) in members.iter().enumerate() {
        let Some(collected) = member.collected else {
            writes.push(None);
            continue;
        };
        // The member's own machine is never listed or authorized on itself.
        let itself = index + 1;
        // The member may know a machine under its own ID, matched by target,
        // and leave it out under that ID.
        let excluded = |machine: &Machine| {
            collected.state.as_ref().is_some_and(|state| {
                machine
                    .ids
                    .iter()
                    .any(|id| state.excluded.contains(id.as_str()))
                    || collected
                        .catalog
                        .ssh
                        .iter()
                        .any(|host| machine.matches(host) && state.excluded.contains(&host.id))
            })
        };
        let mut catalog = collected.catalog.clone();
        let mut devices = 0;
        for (position, machine) in machines.iter().enumerate() {
            if position == itself || excluded(machine) || catalog.ssh.len() >= MAX_HOSTS {
                continue;
            }
            if catalog.ssh.iter().any(|host| machine.matches(host)) {
                continue;
            }
            catalog.ssh.push(machine.host());
            devices += 1;
        }
        let mut keys = Vec::new();
        if !excluded(&machines[0]) {
            keys.push(own_key.line(&state.label));
        }
        for (other, member) in members.iter().enumerate() {
            let Some(key) = member.collected.map(|c| &c.key) else {
                continue;
            };
            if other == index || excluded(&machines[other + 1]) || key == &collected.key {
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
