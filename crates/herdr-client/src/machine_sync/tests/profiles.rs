//! Profiles are matched by target and session, and a catalog never grows
//! past the bytes its reader accepts.
use super::*;
use plan::{Member, plan};

fn ids(hosts: &[SavedHost]) -> Vec<String> {
    hosts.iter().map(|host| host.id.clone()).collect()
}

fn session(mut host: SavedHost, session: &str) -> SavedHost {
    host.session = session.to_owned();
    host
}

fn members<'a>(local: &'a [SavedHost], reports: &'a [remote::Collected]) -> Vec<Member<'a>> {
    local
        .iter()
        .zip(reports)
        .map(|(host, report)| Member {
            host,
            collected: Some(report),
        })
        .collect()
}

#[test]
fn every_session_saved_for_a_target_is_shared() {
    let me = state(1, "me@laptop");
    let local = [host(2, "me@nas"), host(3, "me@gpu")];
    // The NAS saved two sessions of one box.
    let box_default = host(4, "me@box");
    let box_work = session(host(5, "me@box"), "work");
    let reports = [
        collected(2, vec![box_default, box_work], Some(state(2, "me@nas"))),
        collected(3, vec![], Some(state(3, "me@gpu"))),
    ];
    let plan = plan(&me, &key(1), &local, &members(&local, &reports)).unwrap();
    assert_eq!(ids(&plan.learned), [id(4), id(5)]);
    let gpu = plan.writes[1].as_ref().unwrap();
    assert_eq!(ids(&written(gpu)), [id(1), id(2), id(4), id(5)]);
}

#[test]
fn a_members_other_sessions_are_not_listed_on_itself() {
    let me = state(1, "me@laptop");
    // This machine saved two sessions of the NAS, both taking part.
    let local = [host(2, "me@nas"), session(host(3, "me@nas"), "work")];
    let reports = [
        collected(2, vec![], Some(state(2, "me@nas"))),
        collected(2, vec![], Some(state(2, "me@nas"))),
    ];
    let plan = plan(&me, &key(1), &local, &members(&local, &reports)).unwrap();
    for write in &plan.writes {
        assert_eq!(ids(&written(write.as_ref().unwrap())), [id(1)]);
    }
}

#[test]
fn a_catalog_stops_growing_at_its_byte_limit_instead_of_failing() {
    let long = |n: u8| host(n, &format!("me@{}-{n}", "h".repeat(1000)));
    let me = state(1, "me@laptop");
    let local = [host(2, "me@nas"), host(3, "me@gpu")];
    let reports = [
        collected(2, (10..50).map(long).collect(), Some(state(2, "me@nas"))),
        collected(3, (100..120).map(long).collect(), Some(state(3, "me@gpu"))),
    ];
    let plan = plan(&me, &key(1), &local, &members(&local, &reports)).unwrap();
    assert_eq!(plan.learned.len(), 60);
    for (write, had) in plan.writes.iter().zip([40, 20]) {
        let write = write.as_ref().unwrap();
        let listed = written(write);
        assert!(write.catalog.as_ref().unwrap().len() <= 65536);
        assert_eq!(listed.len(), had + write.devices);
        // Each gained some, but not all 61 others: the limit came first.
        assert!(write.devices > 0 && listed.len() < 61, "{}", listed.len());
    }
}
