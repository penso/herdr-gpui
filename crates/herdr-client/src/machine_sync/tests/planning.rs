use super::*;
use plan::{Member, plan};

fn ids(hosts: &[SavedHost]) -> Vec<String> {
    hosts.iter().map(|host| host.id.clone()).collect()
}

#[test]
fn every_member_learns_every_other_machine_and_its_key() {
    let me = state(1, "me@laptop");
    let local = [host(2, "me@nas"), host(3, "me@gpu")];
    let reports = [collected(2, vec![], None), collected(3, vec![], None)];
    let members: Vec<Member> = local
        .iter()
        .zip(&reports)
        .map(|(host, report)| Member {
            host,
            collected: Some(report),
        })
        .collect();
    let plan = plan(&me, &key(1), &local, &members).unwrap();
    assert!(plan.learned.is_empty());
    let nas = plan.writes[0].as_ref().unwrap();
    assert_eq!(ids(&written(nas)), [id(1), id(3)]);
    assert_eq!(nas.devices, 2);
    assert_eq!(
        nas.keys,
        [key(1).line("machine-1"), key(3).line("machine-3")]
    );
    // The member is given a state naming it as this machine saved it.
    let given = parse_state(nas.state.as_deref().unwrap().as_bytes()).unwrap();
    assert_eq!((given.id, given.target), (id(2), "me@nas".to_owned()));
    let gpu = plan.writes[1].as_ref().unwrap();
    assert_eq!(ids(&written(gpu)), [id(1), id(2)]);
    let here: Vec<_> = plan.local_keys.iter().map(|(key, _)| key.clone()).collect();
    assert_eq!(here, [key(2), key(3)]);
}

#[test]
fn a_machine_one_member_lists_is_saved_here_and_given_to_the_others() {
    let me = state(1, "me@laptop");
    let local = [host(2, "me@nas"), host(3, "me@gpu")];
    // The NAS lists a desktop, this machine under its target, and itself.
    let mut laptop = host(8, "me@laptop");
    laptop.label = "Laptop".to_owned();
    let reports = [
        collected(
            2,
            vec![host(4, "me@desk"), laptop],
            Some(state(2, "me@nas")),
        ),
        collected(3, vec![], Some(state(3, "me@gpu"))),
    ];
    let members: Vec<Member> = local
        .iter()
        .zip(&reports)
        .map(|(host, report)| Member {
            host,
            collected: Some(report),
        })
        .collect();
    let plan = plan(&me, &key(1), &local, &members).unwrap();
    assert_eq!(ids(&plan.learned), [id(4)]);
    let nas = plan.writes[0].as_ref().unwrap();
    // It keeps its own list and gains only the GPU.
    assert_eq!(ids(&written(nas)), [id(4), id(8), id(3)]);
    assert_eq!(nas.state, None);
    let gpu = plan.writes[1].as_ref().unwrap();
    assert_eq!(ids(&written(gpu)), [id(1), id(2), id(4)]);
}

#[test]
fn what_a_member_left_out_stays_out_of_its_catalog_keys_and_here() {
    let me = state(1, "me@laptop");
    let local = [host(2, "me@nas"), host(3, "me@gpu")];
    let mut nas_state = state(2, "me@nas");
    nas_state.excluded.extend([id(3), id(5)]);
    let reports = [
        collected(2, vec![host(5, "me@lab")], Some(nas_state)),
        collected(3, vec![], None),
    ];
    let members: Vec<Member> = local
        .iter()
        .zip(&reports)
        .map(|(host, report)| Member {
            host,
            collected: Some(report),
        })
        .collect();
    let plan = plan(&me, &key(1), &local, &members).unwrap();
    assert!(plan.learned.is_empty());
    let nas = plan.writes[0].as_ref().unwrap();
    assert_eq!(ids(&written(nas)), [id(5), id(1)]);
    assert_eq!(nas.keys, [key(1).line("machine-1")]);
}

#[test]
fn a_member_not_read_is_listed_but_its_key_is_not_given() {
    let me = state(1, "me@laptop");
    let local = [host(2, "me@nas"), host(3, "me@gpu")];
    let report = collected(3, vec![], None);
    let members = [
        Member {
            host: &local[0],
            collected: None,
        },
        Member {
            host: &local[1],
            collected: Some(&report),
        },
    ];
    let plan = plan(&me, &key(1), &local, &members).unwrap();
    assert!(plan.writes[0].is_none());
    let gpu = plan.writes[1].as_ref().unwrap();
    assert_eq!(ids(&written(gpu)), [id(1), id(2)]);
    assert_eq!(gpu.keys, [key(1).line("machine-1")]);
    assert_eq!(plan.local_keys.len(), 1);
}

#[test]
fn a_machine_known_by_its_own_id_elsewhere_is_not_listed_twice() {
    let me = state(1, "me@laptop");
    let local = [host(2, "me@nas"), host(3, "me@gpu")];
    // The NAS calls itself 7, and the GPU saved it under 7 and another target.
    let reports = [
        collected(2, vec![], Some(state(7, "me@nas"))),
        collected(3, vec![host(7, "me@nas.lan")], None),
    ];
    let members: Vec<Member> = local
        .iter()
        .zip(&reports)
        .map(|(host, report)| Member {
            host,
            collected: Some(report),
        })
        .collect();
    let plan = plan(&me, &key(1), &local, &members).unwrap();
    assert!(plan.learned.is_empty());
    let gpu = plan.writes[1].as_ref().unwrap();
    assert_eq!(ids(&written(gpu)), [id(7), id(1)]);
}

#[test]
fn a_catalog_with_nothing_missing_is_not_written() {
    let me = state(1, "me@laptop");
    let local = [host(2, "me@nas")];
    let report = collected(2, vec![host(1, "me@laptop")], Some(state(2, "me@nas")));
    let members = [Member {
        host: &local[0],
        collected: Some(&report),
    }];
    let plan = plan(&me, &key(1), &local, &members).unwrap();
    let nas = plan.writes[0].as_ref().unwrap();
    assert_eq!((nas.catalog.as_ref(), nas.devices), (None, 0));
}

#[test]
fn a_full_catalog_gains_nothing() {
    let me = state(1, "me@laptop");
    let local = [host(2, "me@nas")];
    let full: Vec<SavedHost> = (100..100 + MAX_HOSTS as u8)
        .map(|n| host(n, &format!("me@h{n}")))
        .collect();
    let report = collected(2, full, Some(state(2, "me@nas")));
    let members = [Member {
        host: &local[0],
        collected: Some(&report),
    }];
    let plan = plan(&me, &key(1), &local, &members).unwrap();
    assert_eq!(plan.writes[0].as_ref().unwrap().devices, 0);
}
